use std::sync::Arc;
use std::time::Instant;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::{json, Value};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::state_machine::transaction_event_factory::TransactionEventFactory;
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::models::errors::TransactionError;
use foxy_shared::models::transactions::TransactionStatus;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_broadcast_queue_failure, emit_metric};
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::config::get_transaction_event_table;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;
use foxy_shared::services::queue_services::get_sqs_client;
use foxy_shared::utilities::config::get_broadcast_queue;
use foxy_shared::services::queue_services::push_to_broadcast_queue;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;

    let dynamo_db_client = get_dynamodb_client().await;
    log::info!("Committing transaction");

    // Extract values from body
    let transaction_id = body.get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| lambda_http::Error::from("Missing 'transaction_id'"))?;

    let signed_tx = body.get("signed_tx")
        .and_then(|v| v.as_str())
        .ok_or_else(|| lambda_http::Error::from("Missing 'signed_tx'"))?;

    match token{
        None => error_response("Missing authorization token"),
        Some(token) => {
            match handle_signing(token, transaction_id, signed_tx, &dynamo_db_client, &cloudwatch_client).await {
                Ok(..) => {let json = json!({
                        "transaction_id": transaction_id,
                        "status": "Signed",
                        "message": "Transaction signed and queued for broadcast."});

                    success_response(json)
                }
                Err(err) => error_response(format!("{:?}", err)),
            }
        }
    }
}

pub async fn handle_signing(token: &str,
                            transaction_id: &str,
                            signed_tx: &str,
                            dynamo_db_client: &DynamoDbClient,
                            cloudwatch_client: &CloudWatchClient)-> Result<(), TransactionError>
{
    with_valid_user(token, |user_id| async move {
        log::info!("Initiating transaction for user: {}", user_id);
        let start_time = Instant::now();

        let tem = TransactionEventManager::new(Arc::new(dynamo_db_client.clone()), get_transaction_event_table());
        let event = tem.get_latest_event(transaction_id).await?;

        log::info!("Transaction event: {:?}", event);

        //Clone the old tx and add the new data
        let tx = event.transaction
            .clone()
            .with_signed_tx(signed_tx)
            .with_status(TransactionStatus::Signed);

        //Create the signing event
        match TransactionEventFactory::process_event(&event, &tx) {
            Ok(Some(new_event)) => {
                tem.persist(&new_event).await?;
                let sqs_client = get_sqs_client().await?;
                match push_to_broadcast_queue(&sqs_client, &get_broadcast_queue(), transaction_id, &user_id).await{
                    Ok(_) => {},
                    Err(err) => {
                        log::error!("Failed to queue transaction {} for broadcast: {}",transaction_id, err);
                    }
                }
            }
            Ok(None) => {
                log::warn!("No event generated from process_event (possibly idempotent)");
            }
            Err(e) => {
                log::error!("Failed to process transaction event: {:?}", e);
                emit_broadcast_queue_failure(&cloudwatch_client);
                return Err(TransactionError::StateMachine(format!(
                    "process_event failed: {}",
                    e
                )));
            }
        }

        // Log metrics
        let elapsed_time = start_time.elapsed().as_millis() as f64;
        emit_metric(cloudwatch_client, "SigningLatency", elapsed_time, StandardUnit::Milliseconds).await;
        emit_metric(cloudwatch_client, "SignedSuccessCount", 1.0, StandardUnit::Count).await;

        Ok(())

    }).await

}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use foxy_shared::models::transactions::EventType;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};

    #[tokio::test]
    async fn test_signing()  -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let cloudwatch_client = create_cloudwatch_client().await;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");


        // Known test data
        let transaction_id = "146f12e4-a444-49d9-b273-32ead4ec5d67";
        let dummy_signed_tx = "0xf86c808504a817c80082520894abc1234567890abc1234567890abc1234567890088016345785d8a00008026a0b38d3b68eb0ff75aabc...";

        // Act
        match handle_signing(
            &access_token,
            transaction_id,
            dummy_signed_tx,
            &dynamodb_client,
            &cloudwatch_client,
        ).await {
            Ok(_) => {
                println!("✅ Transaction signed successfully");

                // Optional: verify latest event type is Signed
                let event_manager = TransactionEventManager::new(Arc::new(dynamodb_client.clone()), get_transaction_event_table());
                let latest_event = event_manager.get_latest_event(transaction_id).await?;

                assert_eq!(latest_event.event_type, EventType::Signing);
                assert_eq!(latest_event.transaction.signed_tx.as_ref(), Some(&dummy_signed_tx.to_string()));

            }
            Err(e) => {
                println!("❌ Signing failed: {:?}", e);
                panic!("Test failed due to signing error");
            }
        }

        Ok(())
    }
}