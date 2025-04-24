use std::sync::Arc;
use std::time::Instant;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use serde_json::Value;
use foxy_shared::models::transactions::TransactionHistoryItem;
use foxy_shared::models::errors::TransactionError;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::config::get_history_view_table;
use foxy_shared::database::client::get_dynamodb_client;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use serde::{Deserialize, Serialize};
use foxy_shared::views::history_view::TransactionHistoryViewManager;

#[derive(Debug, Deserialize, Serialize)]
struct PagingOptions {
    #[serde(default = "default_limit")]
    limit: i32,
    #[serde(default)]
    next_page_token: Option<String>,
}

fn default_limit() -> i32 { 10 }


pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;
    let dynamo_db_client = get_dynamodb_client().await;
    log::info!("Starting get_recent_transactions");

    match token {
        Some(token) => {
            let paging = match (event.method().as_str(), serde_json::from_value::<PagingOptions>(body.clone())) {
                ("GET", _) => {
                    let query = event.query_string_parameters();
                    let limit = query.first("limit").and_then(|s| s.parse::<i32>().ok()).unwrap_or(10);
                    let next_page_token = query.first("next_page_token").map(String::from);

                    PagingOptions { limit, next_page_token }
                },
                (_, Ok(opts)) => opts,
                (_, Err(_)) => PagingOptions { limit: 10, next_page_token: None },
            };

            match get_transactions(
                token,
                &dynamo_db_client,
                &cloudwatch_client,
                paging,
            ).await {
                Ok(response) => success_response(response),
                Err(err) => error_response(format!("{:?}", err)),
            }
        },
        None => error_response("Missing authorization token"),
    }
}

async fn get_transactions(
    token: &str,
    dynamo_db_client: &DynamoDbClient,
    cloudwatch_client: &CloudWatchClient,
    options: PagingOptions,
) -> Result<Vec<TransactionHistoryItem>, TransactionError> {
    with_valid_user(token, |user_id| async move {
        let start = Instant::now();
        let table_name = get_history_view_table();
        let view = TransactionHistoryViewManager::new(table_name, Arc::new(dynamo_db_client.clone()));

        let start_key = options.next_page_token
            .map(|s| TransactionHistoryViewManager::decode_page_token(&s))
            .transpose()
            .map_err(|e| TransactionError::Projection(format!("Invalid page token: {e}")))?;

        let result = view.query_by_user(&user_id, Some(options.limit), start_key).await
            .map_err(|e| TransactionError::Projection(format!("History query failed: {e}")))?;

        emit_metric(cloudwatch_client, "GetTransactionHistory", start.elapsed().as_millis() as f64, StandardUnit::Milliseconds).await;
        Ok(result.items)
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role, init_tracing};
    use tracing::info;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::config;

    #[tokio::test]
    #[ignore] // requires valid token
    async fn test_get_transactions_live() {
        config::init();
        init_tracing();

        let cognito_client = get_cognito_client_with_assumed_role().await.unwrap();
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;
        let cloudwatch_client = create_cloudwatch_client().await;

        let options = PagingOptions { limit: 5, next_page_token: None };
        let test_user_id = "112527246877271240195";

        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        match get_transactions(
            &access_token,
            &dynamo_db_client,
            &cloudwatch_client,
            options,
        ).await {
            Ok(transactions) => {
                info!(count = transactions.len(), "✅ Retrieved transactions");
                assert!(transactions.len() <= 5);
            },
            Err(e) => panic!("❌ get_transactions failed: {e:?}"),
        }
    }
}
