use std::time::Instant;
use foxy_shared::services::cognito_services::{get_cognito_client, get_user_data, update_user_phone_number};
use foxy_shared::models::phone::PhoneNumber;
use foxy_shared::models::errors::PhoneNumberError;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::phone_numbers::normalize_and_hash;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use foxy_shared::database::dynamo_identity::update_phone_hash;
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{created_response, error_response};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use foxy_shared::database::client::get_dynamodb_client;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let phone_number: Result<PhoneNumber, PhoneNumberError> = serde_json::from_value(body)
        .map_err(|e| PhoneNumberError::InvalidPhoneNumber(format!("{:?}", e)));

    let cloudwatch_client = create_cloudwatch_client().await;
    let client = get_cognito_client().await;
    let dynamodb_client = get_dynamodb_client().await;

    match (token, phone_number) {
        (Some(token), Ok(phone_number)) =>
            match save_phone_number(token, phone_number, &client, &dynamodb_client, &cloudwatch_client).await {
                Ok(response) => created_response(response),
                Err(err) => error_response(format!("{:?}", err)),
            },
        (None, _) => error_response("Missing authorization token"),
        (_, Err(err)) => error_response(format!("{:?}", err)),
    }
}

async fn save_phone_number(token: &str,
                               request: PhoneNumber,
                               cognito_client: &CognitoClient,
                               dynamodb_client: &DynamoDbClient,
                               cloudwatch_client: &CloudWatchClient,) -> Result<String, PhoneNumberError> {
    with_valid_user(token, |user_id| async move {
        let start_time = Instant::now();

        let hashed_phone = normalize_and_hash(&request.number, &request.countrycode)
            .map_err(|e| PhoneNumberError::InvalidPhoneNumber(format!("Failed to normalize phone number: {:?}", e)))?;

        //Cognito update
        update_user_phone_number(cognito_client, &user_id, &hashed_phone)
            .await
            .map_err(|e| PhoneNumberError::CognitoUpdateFailed(format!("Failed to update phone number: {}", e)))?;

        //We also want to copy this users wallet address into the dynamodb table for fast access matching users
        let user_data = get_user_data(cognito_client, &user_id).await;// handle the Result properly
        let user_profile = user_data.map_err(|_| PhoneNumberError::CognitoUpdateFailed("Cannot retrieve user".to_string()))?;

        // handle the Option<String> safely, returning an error if wallet address missing
        let wallet_address = user_profile.wallet_address.as_ref()
            .ok_or_else(|| PhoneNumberError::DynamoDBUpdateFailed("Wallet address does not exist".to_string()))?;


        //DynamoDB update
        update_phone_hash(&dynamodb_client, &hashed_phone, &user_id, wallet_address)
            .await
            .map_err(|e| PhoneNumberError::DynamoDBUpdateFailed(format!("Failed to update phone number: {}", e)))?;

        let duration = start_time.elapsed().as_secs_f64();
        emit_metric(cloudwatch_client, "SavePhoneNumber", duration, "seconds").await;
        Ok("Phone number saved".to_string())
    }).await
}

#[cfg(test)]
mod tests {
    use foxy_shared::services::authentication::generate_tokens;
    use dotenv::dotenv;
    use super::*;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};

    #[tokio::test]
    async fn integration_test() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "112527246877271240195";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");
        let phone_number = foxy_shared::models::phone::PhoneNumber { number: "07533907498".to_string(), countrycode: "GB".to_string() };
        let cloudwatch_client = create_cloudwatch_client().await;

        match save_phone_number(&access_token, phone_number, &cognito_client, &dynamodb_client, &cloudwatch_client).await
        {
            Ok(response) => {
                assert_eq!(response, "Phone number saved");
                Ok(())
            },
            Err(e) => {
                panic!("Expected save, got error: {:?}", e);
            }
        }
}
}