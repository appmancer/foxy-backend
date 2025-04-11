use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use lambda_runtime::{LambdaEvent, Error};
use serde::Deserialize;
use serde_json::Value;
use tracing::{error, info, warn};

use foxy_shared::services::cloudwatch_services::{emit_broadcast_queue_failure, emit_fatality, OperationMetricTracker};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::transactions::TransactionStatus;
use foxy_shared::state_machine::transaction_event_factory::TransactionEventFactory;
use foxy_shared::utilities::config::{get_broadcast_queue, get_rpc_url, get_transaction_event_table};

use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use ethers_core::types::{Bytes, H256};
use ethers_core::utils::keccak256;
use ethers_providers::{Http, Middleware, Provider};
use futures::stream::{FuturesUnordered, StreamExt};

#[derive(Deserialize, Debug)]
struct BroadcastMessage {
    transaction_id: String,
    user_id: String,
}

pub async fn function_handler_with_cache(
    _event: LambdaEvent<Value>,
    recent_tx_hashes: Arc<RwLock<VecDeque<H256>>>,
    sqs_client: &Arc<SqsClient>,
    dynamo_db_client: Arc<DynamoDbClient>
) -> Result<Value, Error> {
    let tracker = OperationMetricTracker::build("BroadcastTriggered").await;
    let queue_url = get_broadcast_queue();
    let tem = Arc::new(TransactionEventManager::new(dynamo_db_client, get_transaction_event_table()));
    let provider = Arc::new(Provider::<Http>::try_from(get_rpc_url())?);

    let result = sqs_client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .visibility_timeout(0)
        .wait_time_seconds(1)
        .send()
        .await?;

    let messages = result
        .messages()
        .iter()
        .filter_map(|msg| {
            let body = msg.body()?;
            let receipt = msg.receipt_handle()?;
            Some((body.to_string(), receipt.to_string()))
        })
        .collect::<Vec<_>>();

    let mut futures = FuturesUnordered::new();
    let mut success_count = 0;
    let mut failure_detected = false;

    for (body, receipt_handle) in messages {
        let parsed_msg: BroadcastMessage = match serde_json::from_str(&body) {
            Ok(msg) => msg,
            Err(e) => {
                error!("❌ Failed to parse broadcast message: {:?}", e);
                continue;
            }
        };

        let tem = Arc::clone(&tem);
        let sqs_client = sqs_client.clone();
        let queue_url = queue_url.clone();
        let provider = Arc::clone(&provider);
        let recent_tx_hashes = Arc::clone(&recent_tx_hashes);

        futures.push(tokio::spawn(async move {
            let event = match tem.get_latest_event(&parsed_msg.transaction_id).await {
                Ok(ev) => ev,
                Err(e) => {
                    error!("❌ Could not get latest event: {:?}", e);
                    return Err(());
                }
            };

            let tx = event.transaction.clone();
            let signed_tx = match tx.signed_tx.as_ref() {
                Some(tx) => tx,
                None => {
                    error!("❌ Transaction missing signed_tx");
                    return Err(());
                }
            };

            let tx_bytes = Bytes::from(match hex::decode(signed_tx.trim_start_matches("0x")) {
                Ok(bytes) => bytes,
                Err(e) => {
                    error!("❌ Could not decode tx: {:?}", e);
                    return Err(());
                }
            });

            let tx_hash = H256::from(keccak256(&tx_bytes));

            {
                let mut hashes = recent_tx_hashes.write().await;
                info!("Checking current hashes");
                if hashes.contains(&tx_hash) {
                    info!("Skipping duplicate tx: {tx_hash:?}");
                    return Ok(());
                }

                // Preemptively reserve the slot
                hashes.push_back(tx_hash);
                if hashes.len() > 10 {
                    hashes.pop_front();
                }
                info!("Hashes length: {}", hashes.len());

            }

            info!("📦 Processing tx {} for user {}", parsed_msg.transaction_id, parsed_msg.user_id);
            info!("🧾 Ready to submit signed_tx for tx_id {}: {}", parsed_msg.transaction_id, signed_tx);

            match provider.send_raw_transaction(tx_bytes.clone()).await {
                Ok(pending) => {
                    info!("✅ Broadcasted to Optimism with tx hash: {:#x}", pending.tx_hash());

                    let updated_tx = tx
                        .clone()
                        .with_status(TransactionStatus::Pending)
                        .with_transaction_hash(&format!("{:#x}", pending.tx_hash()));

                    if let Some(new_event) = TransactionEventFactory::process_event(&event, &updated_tx).unwrap_or(None) {
                        tem.persist_dual(&new_event).await.ok();
                        info!("📝 Broadcast event persisted");
                    }

                    delete_sqs_message(&sqs_client, &queue_url, &receipt_handle).await;
                    Ok(())
                }
                Err(e) => {
                    warn!("⚠️ Broadcast failed: {:?}", e);

                    if let Ok(Some(tx)) = provider.get_transaction(tx_hash).await {
                        info!("🟢 Tx already on-chain: {:#x}", tx.hash);
                        delete_sqs_message(&sqs_client, &queue_url, &receipt_handle).await;
                        return Ok(());
                    }

                    emit_fatality(&foxy_shared::services::cloudwatch_services::create_cloudwatch_client().await, "OptimismBroadcast").await;
                    Err(())
                }
            }
        }));
    }

    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok(())) => success_count += 1,
            Ok(Err(_)) | Err(_) => failure_detected = true,
        }
    }

    if failure_detected {
        emit_broadcast_queue_failure(&foxy_shared::services::cloudwatch_services::create_cloudwatch_client().await);
    }

    tracker.track::<(), Box<dyn std::error::Error>>(&Ok(()), Some(success_count as f64)).await;

    Ok(serde_json::json!({
        "message": "Hello from foxy broadcaster!",
        "messages_processed": success_count,
        "success_count": success_count
    }))
}

async fn delete_sqs_message(sqs_client: &SqsClient, queue_url: &str, receipt_handle: &str) {
    info!("Deleting from queue {}", queue_url);
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
