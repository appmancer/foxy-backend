use aws_sdk_cognitoidentityprovider::Client;
use aws_sdk_cognitoidentityprovider::types::AuthFlowType;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fmt;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::logging::log_info;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::Body;
use serde_json::Value;
use foxy_shared::utilities::responses::{error_response, success_response};

#[derive(Serialize, Deserialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug)]
pub enum RefreshError {
    MissingRefreshToken,
    CognitoAuthFailed(String),
}


impl fmt::Display for RefreshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefreshError::MissingRefreshToken => write!(f, "Missing refresh token"),
            RefreshError::CognitoAuthFailed(reason) => write!(f, "Cognito auth failed: {}", reason),
        }
    }
}
pub async fn handler(body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let cloudwatch_client = create_cloudwatch_client().await;
    match body.get("refresh_token").and_then(|v| v.as_str()) {
        Some(token) => match refresh_access_token(token, &cloudwatch_client).await {
            Ok(response) => success_response(response),
            Err(err) => error_response(format!("{:?}", err)),
        },
        None => error_response("Missing refresh_token"),
    }
}

async fn refresh_access_token(
    refresh_token: &str,
    cloudwatch_client: &CloudWatchClient
) -> Result<RefreshResponse, RefreshError> {
    log_info("refresh_access_token", "Attempting to refresh access token");

    if refresh_token.is_empty() {
        return Err(RefreshError::MissingRefreshToken);
    }

    // Load AWS configuration and initialize Cognito client
    let config = aws_config::load_from_env().await;
    let client = Client::new(&config);

    let user_pool_client_id = std::env::var("USER_POOL_CLIENT_ID")
        .map_err(|_| RefreshError::CognitoAuthFailed("Missing USER_POOL_CLIENT_ID environment variable".to_string()))?;

    let mut auth_params = HashMap::new();
    auth_params.insert("REFRESH_TOKEN".to_string(), refresh_token.to_string());

    let start_time = std::time::Instant::now();

    let response = client
        .admin_initiate_auth()
        .client_id(&user_pool_client_id)
        .auth_flow(AuthFlowType::RefreshTokenAuth)
        .set_auth_parameters(Some(auth_params))
        .send()
        .await
        .map_err(|err| RefreshError::CognitoAuthFailed(format!("AWS Cognito error: {:?}", err)))?;

    let elapsed_time = start_time.elapsed().as_millis() as f64;
    emit_metric(cloudwatch_client,"TokenRefreshLatency", elapsed_time, StandardUnit::Milliseconds).await;

    let access_token = response
        .authentication_result()
        .and_then(|result| result.access_token())
        .map(|token| token.to_string())
        .ok_or_else(|| RefreshError::CognitoAuthFailed("Missing access token".to_string()))?;

    let expires_in = response
        .authentication_result()
        .and_then(|result| Some(result.expires_in()))
        .map(|expiry| expiry as u64)
        .ok_or_else(|| RefreshError::CognitoAuthFailed("Missing expires_in".to_string()))?;

    emit_metric(cloudwatch_client, "TokenRefreshSuccess", 1.0, StandardUnit::Count).await;

    Ok(RefreshResponse {
        access_token,
        expires_in,
    })
}
