use std::time::Instant;
use http::Response;
use lambda_http::{Body, Request};
use foxy_shared::services::cognito_services::{get_cognito_client, get_user_data};
use foxy_shared::models::errors::WalletError;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;

pub async fn handler(event: Request) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;

    match token {
        Some(token) => match fetch_wallet(token, &cloudwatch_client).await {
            Ok(response) => success_response(response),
            Err(err) => error_response(format!("{:?}", err)),
        },
        _ => error_response("Missing authorization token"),
    }
}

async fn fetch_wallet(token: &str, cloudwatch_client: &CloudWatchClient) -> Result<String, WalletError> {
    with_valid_user(token, |user_id| async move {
        log::info!("Fetching wallet for user: {}", user_id);
        let start_time = Instant::now();
        let client = get_cognito_client().await;

        let user_profile = get_user_data(&client, &user_id)
            .await
            .map_err(|e| WalletError::MissingWallet(format!("Failed to fetch user data: {:?}", e)))?;

        let duration = start_time.elapsed().as_secs_f64();
        emit_metric(cloudwatch_client, "FetchWallet", duration, StandardUnit::Seconds).await;
        Ok(user_profile.wallet_address.unwrap())
    }).await
}