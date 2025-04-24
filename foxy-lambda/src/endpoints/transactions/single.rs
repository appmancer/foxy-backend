use std::sync::Arc;
use std::time::Instant;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::StandardUnit;
use http::{Response, StatusCode};
use lambda_http::{Body, Request};
use foxy_shared::models::transactions::TransactionHistoryItem;
use foxy_shared::models::errors::TransactionError;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, response_with_code, success_response};
use foxy_shared::utilities::config::get_history_view_table;
use foxy_shared::database::client::get_dynamodb_client;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use foxy_shared::views::history_view::TransactionHistoryViewManager;

pub async fn handler(event: Request, bundle_id: &str) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;
    let dynamo_db_client = get_dynamodb_client().await;
    log::info!("Starting get_single_transaction");

    match token {
        Some(token) => {
            match get_transaction(
                token,
                &dynamo_db_client,
                &cloudwatch_client,
                bundle_id
            ).await {
                Ok(response) => success_response(response),
                Err(TransactionError::NotFound(msg)) => response_with_code(&msg, StatusCode::NOT_FOUND),
                Err(err) => error_response(format!("{:?}", err)),
            }
        },
        None => error_response("Missing authorization token"),
    }
}

async fn get_transaction(
    token: &str,
    dynamo_db_client: &DynamoDbClient,
    cloudwatch_client: &CloudWatchClient,
    bundle_id: &str,
) -> Result<TransactionHistoryItem, TransactionError> {
    with_valid_user(token, |user_id| async move {
        let start = Instant::now();
        let table_name = get_history_view_table();
        let view = TransactionHistoryViewManager::new(table_name, Arc::new(dynamo_db_client.clone()));

        let result = view.get_by_bundle_id_for_user(&user_id, bundle_id).await
            .map_err(|e| TransactionError::Projection(format!("History lookup failed: {e}")))?;

        emit_metric(cloudwatch_client, "GetTransactionById", start.elapsed().as_millis() as f64, StandardUnit::Milliseconds).await;

        result.ok_or_else(|| TransactionError::NotFound(format!("No transaction found for bundle {}", bundle_id)))
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role, init_tracing};
    use foxy_shared::services::cloudwatch_services::create_cloudwatch_client;
    use tracing::info;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::config;

    #[tokio::test]
    #[ignore]
    async fn test_get_transaction_by_id_live() {
        config::init();
        init_tracing();

        let cognito_client = get_cognito_client_with_assumed_role().await.unwrap();
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;
        let cloudwatch_client = create_cloudwatch_client().await;

        let test_user_id = "112527246877271240195";
        let bundle_id = "112527246877271240195";

        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        match get_transaction(&access_token, &dynamo_db_client, &cloudwatch_client, &bundle_id).await {
            Ok(item) => {
                info!(?item, "✅ Retrieved transaction by bundle ID");
                assert_eq!(item.bundle_id, bundle_id);
            },
            Err(e) => panic!("❌ get_transaction failed: {e:?}"),
        }
    }
}
