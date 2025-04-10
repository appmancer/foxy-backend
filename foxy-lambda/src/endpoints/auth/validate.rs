use serde::Serialize;
use serde_json::Value;
use foxy_shared::utilities::config;
use foxy_shared::models::errors::ValidateError;
use foxy_shared::services::cognito_services::{get_user_data, get_cognito_client, check_user_exists, create_user_and_set_password};
use foxy_shared::services::authentication;
use foxy_shared::services::authentication::generate_tokens;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use http::Response;
use lambda_http::Body;
use foxy_shared::utilities::responses::{error_response, success_response};

#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub message: String,
    pub sub: String,
    pub name: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub wallet_address: String,
    pub phone_hash: String,
    pub default_currency: String
}


pub async fn handler(body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let cloudwatch_client = create_cloudwatch_client().await;
    match validate(body, &cloudwatch_client).await {
        Ok(response) => success_response(response),
        Err(err) => error_response(format!("{}", err)),
    }
}

async fn validate(event_body: Value, cloudwatch_client: &CloudWatchClient) -> Result<ValidateResponse, ValidateError> {
    log::info!("Received a validate request.");

    let id_token = event_body.get("id_token")
        .and_then(|v| v.as_str())
        .ok_or(ValidateError::MissingIdToken)?;

    let start_time = std::time::Instant::now();  // Track time for performance monitoring

    config::init();
    let client_id = config::get_google_client_id();

    let valid_claims = authentication::validate_id_token(id_token, &client_id).await?;
    let sub = valid_claims.sub.clone();
    let name = valid_claims.name.clone().unwrap_or_else(|| "Unknown".to_string());
    let email = if valid_claims.email.is_empty() {
        "No email provided".to_string()
    } else {
        valid_claims.email.clone()
    };
    let phone_number: Option<String> = None; // `phone_number` does not exist in GoogleClaims, so default to None or remove this line

    log::info!("Phone: {}", phone_number.clone().unwrap_or_default());

    check_or_create_cognito_user(&sub, &name, Some(email.as_str()), phone_number.as_deref()).await?;

    let client = get_cognito_client().await;
    let tokens = generate_tokens(&client, &sub).await?;

    // Fetch user attributes in a single query
    let user_profile = get_user_data(&client, &sub)
        .await
        .map_err(|e| ValidateError::CognitoCheckFailed(format!("Failed to fetch user data: {:?}", e)))?;

    emit_metric(cloudwatch_client,"AuthValidationSuccess", 1.0, "Count").await;
    emit_metric(cloudwatch_client,"ValidationLatency", start_time.elapsed().as_millis() as f64, "Milliseconds").await;

    Ok(ValidateResponse {
        message: "Token is valid, user exists (or was created) in Cognito".to_string(),
        sub,
        name,
        email,
        access_token: tokens.access_token.unwrap_or_default(),
        refresh_token: tokens.refresh_token.unwrap_or_default(),
        id_token: tokens.id_token.unwrap_or_default(),
        wallet_address: user_profile.wallet_address.unwrap_or_default(),
        phone_hash: user_profile.phone_hash.unwrap_or_default(),
        default_currency: user_profile.currency.unwrap_or("GBP".to_string()),
    })
}

async fn check_or_create_cognito_user(sub: &str, name: &str, email: Option<&str>, phone: Option<&str>) -> Result<bool, ValidateError> {
    let client = get_cognito_client().await;

    match check_user_exists(&client, sub).await {
        Ok(true) => Ok(true),
        Ok(false) => {
            log::info!("User does not exist. Creating...");
            create_user_and_set_password(&client, &sub, email, name, phone)
                .await
                .map_err(|err| ValidateError::CognitoCheckFailed(err.to_string()))?;
            Ok(true)
        }
        Err(err) => Err(ValidateError::CognitoCheckFailed(err.to_string())),
    }
}
