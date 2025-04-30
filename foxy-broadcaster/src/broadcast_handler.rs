use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::Deserialize;
use tracing::{error, info, warn};
use foxy_shared::services::cloudwatch_services::{emit_broadcast_queue_failure, OperationMetricTracker};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::transactions::{BundleStatus, EventType, TransactionEvent, TransactionLeg};
use foxy_shared::utilities::config::{get_broadcast_queue, get_rpc_url, get_transaction_event_table};

use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use ethers_core::types::{Bytes, H256};
use ethers_core::utils::keccak256;
use ethers_providers::{Http, Middleware, Provider};
use futures::stream::{FuturesUnordered, StreamExt};
use lambda_http::{Response, Request, Body};
use foxy_shared::utilities::responses::{error_response, success_response};
use anyhow::Error as AnyError;

#[derive(Deserialize, Debug)]
struct BroadcastMessage {
    bundle_id: String,
    user_id: String,
}

pub async fn handle_request(request: Request,
                            recent_tx_hashes: Arc<RwLock<VecDeque<H256>>>,
                            sqs_client: &Arc<SqsClient>,
                            dynamo_db_client: Arc<DynamoDbClient>,) -> Result<Response<Body>, lambda_http::Error> {
    match function_handler_with_cache(request,
                                      recent_tx_hashes,
                                      sqs_client,
                                      dynamo_db_client).await {
        Ok(count) => success_response(count.to_string()),
        Err(e) => error_response(format!("Broadcast failed: {}", e)),
    }
}

pub async fn function_handler_with_cache(
    _request: Request,
    recent_tx_hashes: Arc<RwLock<VecDeque<H256>>>,
    sqs_client: &Arc<SqsClient>,
    dynamo_db_client: Arc<DynamoDbClient>,
) -> Result<u32, AnyError> {

    info!("Starting broadcast handler");
    info!("📍 AWS_REGION = {:?}", std::env::var("AWS_REGION"));
    info!("📍 AWS_ACCESS_KEY_ID = {:?}", std::env::var("AWS_ACCESS_KEY_ID"));
    info!("📍 BROADCAST_QUEUE_URL = {:?}", std::env::var("BROADCAST_QUEUE_URL"));

    let tracker = Arc::new(OperationMetricTracker::build("BroadcastTriggered").await);

    let queue_url = get_broadcast_queue();
    info!("📦 Queue URL resolved: {}", queue_url);

    let tem = TransactionEventManager::new(dynamo_db_client, get_transaction_event_table());
    let provider = Arc::new(Provider::<Http>::try_from(get_rpc_url())?);

    let result = match sqs_client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .visibility_timeout(0)
        .wait_time_seconds(1)
        .send()
        .await
    {
        Ok(res) => {
            info!("📥 Pulled messages from SQS");
            res
        }
        Err(e) => {
            error!("❌ Failed to receive SQS message: {:?}", e);
            return Err(e.into());
        }
    };

    let messages = result
        .messages()
        .iter()
        .filter_map(|msg| {
            let body = msg.body()?;
            let receipt = msg.receipt_handle()?;
            Some((body.to_string(), receipt.to_string()))
        })
        .collect::<Vec<_>>();
    tracing::info!("📦 Total messages received: {}", messages.len());

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

        info!("📨 Raw message body: {}", body);

        let tem = Arc::clone(&tem);
        let sqs_client = sqs_client.clone();
        let queue_url = queue_url.clone();
        let provider = Arc::clone(&provider);
        let recent_tx_hashes = Arc::clone(&recent_tx_hashes);
        let tracker_for_loop = tracker.clone();

        futures.push(tokio::spawn(async move {
            let last_event = match tem.get_latest_event(&parsed_msg.bundle_id).await {
                Ok(ev) => ev,
                Err(e) => {
                    error!("❌ Could not get latest event: {:?}", e);
                    return Err(());
                }
            };

            let (leg, signing_data) = match (&last_event.event_type, &last_event.bundle_snapshot.status) {
                (EventType::Sign, BundleStatus::Signed) => (TransactionLeg::Main, last_event.bundle_snapshot.main_tx.signed_tx.clone()),
                (EventType::Confirm, BundleStatus::MainConfirmed) => (TransactionLeg::Fee, last_event.bundle_snapshot.fee_tx.signed_tx.clone()),
                _ => {
                    error!("{}", format!("Cannot broadcast from EventType:{} and BundleStatus:{}",
                            &last_event.event_type, &last_event.bundle_snapshot.status));
                    return Err(());
                }
            };

            let signing_data = signing_data.ok_or_else(|| {
                error!("Missing signing data on event id: {}", &last_event.event_id);
                ()
            })?;

            info!("📨 Signing Data: {}", signing_data);

            info!("Broadcasting signing data: {}", signing_data.to_string());
            let tx_bytes = Bytes::from(match hex::decode(signing_data.trim_start_matches("0x")) {
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
                    let _ = &tracker_for_loop.emit("DuplicateTxSkipped", 1.0, "Count", &[]).await;
                    return Ok(());
                }

                // Preemptively reserve the slot
                hashes.push_back(tx_hash);
                if hashes.len() > 10 {
                    hashes.pop_front();
                }
                info!("Hashes length: {}", hashes.len());
            }

            info!("📦 Processing bundle {} for user {}", parsed_msg.bundle_id, parsed_msg.user_id);
            info!("🧾 Ready to submit signed_tx for bundle {}: {}", parsed_msg.bundle_id, signing_data);

            match provider.send_raw_transaction(tx_bytes.clone()).await {
                Ok(pending) => {
                    info!("✅ Broadcasted to Optimism with tx hash: {:#x}", pending.tx_hash());

                    info!("📌 Emitting Broadcast event for bundle {} with tx_hash: {:#x}", last_event.bundle_id, tx_hash);
                    match TransactionEvent::on_broadcast(&last_event, tx_hash, tem.clone()).await {
                        Ok(_) => {
                            info!("📦 Broadcast event successfully recorded for bundle {}", last_event.bundle_id);
                        }
                        Err(e) => {
                            error!("❌ Failed to emit Broadcast event for bundle {}: {:?}", last_event.bundle_id, e);
                        }
                    }

                    delete_sqs_message(&sqs_client, &queue_url, &receipt_handle).await;
                    Ok(())
                }
                Err(e) => {
                    warn!("⚠️ Broadcast failed: {:?}", e);

                    // Check if the tx is already on-chain before failing
                    match provider.get_transaction(tx_hash).await {
                        Ok(Some(tx)) => {
                            info!("🟢 Tx already on-chain: {:#x}", tx.hash);
                            delete_sqs_message(&sqs_client, &queue_url, &receipt_handle).await;
                            return Ok(());
                        }
                        Ok(None) => {
                            warn!("🔍 Tx not found on-chain, proceeding with failure handling");
                        }
                        Err(err) => {
                            warn!("⚠️ Could not check on-chain tx status: {:?}", err);
                        }
                    }

                    let _ = TransactionEvent::on_fail(&last_event, leg, tem).await;
                    delete_sqs_message(&sqs_client, &queue_url, &receipt_handle).await;
                    tracker_for_loop.emit_fatal("OptimismBroadcast").await;
                    Err(())
                }
            }
        }));
    }

    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok(())) => success_count += 1,
            Ok(Err(_)) => {
                failure_detected = true;
                error!("❌ Broadcast worker returned failure inside Ok(Err(_))");
            }
            Err(e) => {
                failure_detected = true;
                error!("❌ Broadcast worker panicked or join error: {:?}", e);
            }
        }
    }


    if failure_detected {
        emit_broadcast_queue_failure(&foxy_shared::services::cloudwatch_services::create_cloudwatch_client().await);
        return Err(AnyError::msg("Broadcast failure detected"));
    }

    tracker.track::<(), Box<dyn std::error::Error + Send + Sync>>(&Ok(()), Some(success_count as f64)).await;


    Ok(success_count)
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
