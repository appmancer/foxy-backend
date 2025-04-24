use std::sync::Arc;
use std::time::Instant;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::{json, Value};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::models::errors::TransactionError;
use foxy_shared::models::transactions::TransactionEvent;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_broadcast_queue_failure, emit_metric};
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::config::get_transaction_event_table;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;
use foxy_shared::services::queue_services::{get_sqs_client, push_to_broadcast_queue};
use foxy_shared::utilities::config::get_broadcast_queue;
use crate::models::transactions::{SignedTransactionError, SignedTransactionPayload};

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;

    let dynamo_db_client = get_dynamodb_client().await;
    log::info!("Committing transaction");

    let payload: Result<SignedTransactionPayload, SignedTransactionError> = serde_json::from_value(body)
        .map_err(|e| SignedTransactionError::InvalidPayload(format!("{:?}", e)));

    match token{
        None => error_response("Missing authorization token"),
        Some(token) => {
            match handle_signing(token, &payload.unwrap(), &dynamo_db_client, &cloudwatch_client).await {
                Ok(new_event) => {
                    let id = new_event.bundle_id.clone();
                    let json = json!({
                        "bundle_id": id,
                        "status": new_event.bundle_status,
                        "message": "Transaction signed and queued for broadcast."});

                    success_response(json)
                }
                Err(err) => error_response(format!("{:?}", err)),
            }
        }
    }
}

pub async fn handle_signing(token: &str,
                            payload: &SignedTransactionPayload,
                            dynamo_db_client: &DynamoDbClient,
                            cloudwatch_client: &CloudWatchClient)-> Result<TransactionEvent, TransactionError>
{
    with_valid_user(token, |user_id| async move {
        log::info!("Initiating transaction for user: {}", user_id);
        let start_time = Instant::now();

        let tem = TransactionEventManager::new(Arc::new(dynamo_db_client.clone()), get_transaction_event_table());
        let event = tem.get_latest_event(&payload.bundle_id).await?;

        let new_event = match TransactionEvent::on_signed(&event,
                                                                   &payload.fee_signed_tx,
                                                                   &payload.main_signed_tx,
                                                                   tem).await {
            Ok(ev) => ev,
            Err(e) => { return Err(e) }
        };

        log::info!("new transaction event: {:?}", &new_event);
        
        let sqs_client = get_sqs_client().await?;
        match push_to_broadcast_queue(&sqs_client, &get_broadcast_queue(), &new_event.bundle_id, &user_id).await{
            Ok(_) => {},
            Err(err) => {
                emit_broadcast_queue_failure(&cloudwatch_client);
                log::error!("Failed to queue transaction {} for broadcast: {}", &new_event.bundle_id, err);
            }
        }

        // Log metrics
        let elapsed_time = start_time.elapsed().as_millis() as f64;
        emit_metric(cloudwatch_client, "SigningLatency", elapsed_time, StandardUnit::Milliseconds).await;
        emit_metric(cloudwatch_client, "SignedSuccessCount", 1.0, StandardUnit::Count).await;

        Ok(new_event)

    }).await

}

#[cfg(test)]
mod tests {
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::config;
    use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;

    #[tokio::test]
    async fn test_signing()  -> Result<(), Box<dyn std::error::Error>> {
        config::init();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let _access_token = token_result.access_token.expect("Access token missing");

        Ok(())
    }
}