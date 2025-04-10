use aws_sdk_sqs::{Client as SqsClient};
use lambda_runtime::{LambdaEvent, Error};
use serde_json::{Value};
use serde::Deserialize;
use tracing::{info, error};
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::transactions::TransactionStatus;
use foxy_shared::services::queue_services::get_sqs_client;
use foxy_shared::state_machine::transaction_event_factory::TransactionEventFactory;
use foxy_shared::utilities::config::{get_broadcast_queue, get_ethereum_url, get_transaction_event_table};
use ethers_providers::{Provider, Http, Middleware};
use ethers_core::types::Bytes;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_broadcast_queue_failure, emit_fatality, OperationMetricTracker};
use foxy_shared::track_ok;

pub async fn function_handler(_event: LambdaEvent<Value>) -> Result<Value, Error> {

    let tracker = OperationMetricTracker::build("BroadcastTriggered").await;

    track_ok!(tracker, async {
        let sqs_client = get_sqs_client().await.unwrap();
        let queue_url = get_broadcast_queue();
        let messages = peek_queue(&sqs_client, &queue_url).await?;

        let mut success_count = 0;
        let mut failure_detected = false;

        for (body, receipt_handle) in &messages {
            match process_single_message(&sqs_client, &queue_url, body, receipt_handle).await {
                Ok(_) => success_count += 1,
                Err(_) => failure_detected = true,
            }
        }

        if failure_detected {
            emit_broadcast_queue_failure(&create_cloudwatch_client().await);
        }

        tracker.track::<(), Box<dyn std::error::Error>>(
            &Ok(()),                      // Always return Ok — metric only cares about latency
            Some(success_count as f64),  // Emit count of successfully processed messages
        ).await;

        Ok(serde_json::json!({
            "message": "Hello from foxy broadcaster!",
            "messages_processed": messages.len(),
            "success_count": success_count
        }))
    })
}

async fn process_single_message(
    sqs_client: &SqsClient,
    queue_url: &str,
    body: &str,
    receipt_handle: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("📬 Received message: {}", body);

    let parsed_msg: BroadcastMessage = serde_json::from_str(body)?;
    info!(
        "📦 Processing tx {} for user {}",
        parsed_msg.transaction_id, parsed_msg.user_id
    );

    match handle_broadcast_message(&parsed_msg).await {
        Ok(_) => {
            delete_sqs_message(sqs_client, queue_url, receipt_handle).await;
            Ok(())
        }
        Err(e) => {
            emit_fatality(&create_cloudwatch_client().await, "BroadcastFailure").await;
            Err(e)
        }
    }
}

#[derive(Deserialize, Debug)]
struct BroadcastMessage {
    transaction_id: String,
    user_id: String,
}
async fn delete_sqs_message(
    sqs_client: &SqsClient,
    queue_url: &str,
    receipt_handle: &str,
) {
    if let Err(e) = sqs_client
        .delete_message()
        .queue_url(queue_url)
        .receipt_handle(receipt_handle)
        .send()
        .await
    {
        error!("❌ Failed to delete SQS message: {:?}", e);
    } else {
        info!("✅ Deleted SQS message with receipt handle: {}", receipt_handle);
    }
}

/// Pull messages from the broadcast queue
async fn peek_queue(
    sqs_client: &SqsClient,
    queue_url: &str,
) -> Result<Vec<(String, String)>, Error> {
    let result = sqs_client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(10)
        .visibility_timeout(0)
        .wait_time_seconds(1)
        .send()
        .await
        .expect("Failed to receive messages");

    Ok(result
        .messages()
        .iter()
        .filter_map(|msg| {
            let body = msg.body()?;
            let receipt = msg.receipt_handle()?;
            Some((body.to_string(), receipt.to_string()))
        })
        .collect())
}

async fn handle_broadcast_message(
    msg: &BroadcastMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let dynamo_db_client = get_dynamodb_client().await;
    let tem = TransactionEventManager::new(&dynamo_db_client, get_transaction_event_table());

    let event = tem.get_latest_event(&msg.transaction_id).await?;
    let tx = event.transaction.clone();

    let signed_tx = tx
        .signed_tx
        .as_ref()
        .ok_or("Transaction is missing signed_tx")?;

    // 🔥 Actually send it to Optimism
    let tx_hash = submit_to_optimism(signed_tx).await?;

    // 🔄 Update the tx object
    let updated_tx = tx
        .clone()
        .with_status(TransactionStatus::Broadcasted)
        .with_transaction_hash(tx_hash.as_str());

    // 🧠 Drive state machine
    if let Some(new_event) = TransactionEventFactory::process_event(&event, &updated_tx)? {
        tem.persist_dual(&new_event).await?;
        log::info!("📝 Broadcast event persisted");
    } else {
        log::warn!("⚠️ No new event emitted (likely idempotent)");
    }

    Ok(())
}

pub async fn submit_to_optimism(signed_tx: &str) -> Result<String, Box<dyn std::error::Error>> {
    let tracker = OperationMetricTracker::build("BroadcastToNetwork").await;

    track_ok!(tracker, async {
        let rpc_url = get_ethereum_url();

        let provider = Provider::<Http>::try_from(rpc_url)?;
        let tx_bytes = Bytes::from(hex::decode(signed_tx.trim_start_matches("0x"))?);

        let send_result = provider.send_raw_transaction(tx_bytes).await;

        match send_result{
            Ok(pending) => {
                let tx_hash = format!("{:#x}", pending.tx_hash());
                log::info!("✅ Broadcasted to Optimism with tx hash: {}", tx_hash);
                Ok(tx_hash)
            }
            Err(e) => {
                log::error!("🚨 Failed to send tx to Optimism: {:?}", e);
                emit_fatality(&create_cloudwatch_client().await, "OptimismBroadcast").await;
                Err(e.into())
            }
        }
    })
}