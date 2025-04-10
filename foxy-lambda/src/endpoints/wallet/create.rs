use std::time::Instant;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::services::cognito_services::{get_cognito_client, get_user_data, update_user_wallet_address};
use foxy_shared::utilities::token_validation::validate_cognito_token;
use foxy_shared::utilities::config;
use foxy_shared::utilities::logging::log_info;
use foxy_shared::models::errors::WalletError;
use foxy_shared::models::wallet::WalletCreateResponse;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use foxy_shared::utilities::authentication::with_valid_user;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;
    let wallet_address = body.get("walletAddress").and_then(|v| v.as_str());

    match (token, wallet_address) {
        (Some(token), Some(wallet_address)) => match create_wallet(token, wallet_address, &cloudwatch_client).await {
            Ok(response) => success_response(response),
            Err(err) => error_response(format!("{:?}", err)),
        },
        (None, _) => error_response("Missing authorization token"),
        (_, None) => error_response("Missing wallet address"),
    }
}

async fn create_wallet(token: &str, wallet_address: &str, cloudwatch_client: &CloudWatchClient)
                            -> Result<WalletCreateResponse, WalletError> {
    with_valid_user(token, |_| async move {
        let start_time = Instant::now();

        let user_pool_id = config::get_user_pool_id();
        let region = config::get_aws_region();
        let claims = validate_cognito_token(token, &user_pool_id, &region)
            .await
            .map_err(|e| WalletError::InvalidToken(format!("{:?}", e)))?;
        let user_id = claims.username;

        log_info("wallet_creation", &format!("User validated: {}", user_id));

        // Validate the wallet address format
        if !wallet_address.starts_with("0x") || wallet_address.len() != 42 {
            return Err(WalletError::InvalidWalletAddress);
        }

        let client = get_cognito_client().await;

        let user_profile = get_user_data(&client, &user_id)
            .await
            .map_err(|e| WalletError::CognitoUpdateFailed(format!("Failed to fetch user data: {:?}", e)))?;

        if user_profile.wallet_address.is_some() {
            return Err(WalletError::WalletAlreadyExists);
        }

        // Update Cognito with the new wallet address
        update_user_wallet_address(&client, &user_id, wallet_address)
            .await
            .map_err(|e| WalletError::CognitoUpdateFailed(format!("Failed to update wallet address: {}", e)))?;

        log_info("wallet_creation", "Wallet successfully created");

        let duration = start_time.elapsed().as_secs_f64();
        emit_metric(cloudwatch_client, "CreateWallet", duration, "seconds").await;
        Ok(WalletCreateResponse {
            message: "Wallet address successfully added".to_string(),
        })
    }).await
}