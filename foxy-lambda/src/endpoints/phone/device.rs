use std::time::Instant;
use foxy_shared::models::errors::DeviceError;
use foxy_shared::utilities::authentication::with_valid_user;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{created_response, error_response};
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::models::user_device::UserDevice;
use foxy_shared::utilities::config::get_user_device_table;
use foxy_shared::services::user_device_service::UserDeviceService;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let device: Result<UserDevice, DeviceError> = serde_json::from_value(body)
        .map_err(|e| DeviceError::ParseError(format!("{:?}", e)));

    let cloudwatch_client = create_cloudwatch_client().await;
    let dynamodb_client = get_dynamodb_client().await;

    match (token, device) {
        (Some(token), Ok(device)) =>
            match save_device(token, &device, dynamodb_client, &cloudwatch_client).await {
                Ok(response) => created_response(response),
                Err(err) => error_response(format!("{:?}", err)),
            },
        (None, _) => error_response("Missing authorization token"),
        (_, Err(err)) => error_response(format!("{:?}", err)),
    }
}

async fn save_device(token: &str,
                     device: &UserDevice,
                     dynamodb_client: DynamoDbClient,
                     cloudwatch_client: &CloudWatchClient,) -> Result<String, DeviceError> {
    with_valid_user(token, |user_id| async move {
        let start_time = Instant::now();

        let complete_device = UserDevice::new(
            device.device_fingerprint.clone(),
            device.push_token.clone(),
            device.platform.clone(),
            device.app_version.clone());

        let service = UserDeviceService::new(dynamodb_client, get_user_device_table());
        service.store_device(&user_id, &complete_device).await?;

        let duration = start_time.elapsed().as_secs_f64();
        emit_metric(cloudwatch_client, "StoreDevice", duration, StandardUnit::Seconds).await;
        Ok("Device saved".to_string())
    }).await
}