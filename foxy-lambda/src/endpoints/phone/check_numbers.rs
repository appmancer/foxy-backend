use std::time::Instant;
use foxy_shared::models::phone::{PhoneCheckRequest, PhoneCheckResponse};
use foxy_shared::models::errors::PhoneNumberError;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::phone_numbers::normalize_and_hash;
use foxy_shared::database::dynamo_identity::parallel_batches;
use aws_sdk_dynamodb::{Client as DynamoDbClient};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let request: Result<PhoneCheckRequest, PhoneNumberError> = serde_json::from_value(body)
        .map_err(|e| PhoneNumberError::InvalidPhoneNumber(format!("{:?}", e)));

    match (token, &request) {
        (Some(token), Ok(parsed_request)) => {
            //log::info!("Checking phone numbers: {:?}", &parsed_request.phone_numbers);
            let dynamodb_client = get_dynamodb_client().await;
            let cloudwatch_client = create_cloudwatch_client().await;

            match check_phone_numbers(token, parsed_request, &dynamodb_client, &cloudwatch_client).await {
                Ok(response) => success_response(response),
                Err(err) => error_response(format!("{:?}", err)),
            }
        },
        (None, _) => return error_response("Missing authorization token"),
        (_, Err(err)) => return error_response(format!("{:?}", err)),
    }
}


async fn check_phone_numbers(
    token: &str,
    request: &PhoneCheckRequest,
    dynamodb_client: &DynamoDbClient,
    cloudwatch_client: &CloudWatchClient,
) -> Result<Vec<PhoneCheckResponse>, PhoneNumberError> {
    with_valid_user(token, |_| async move {
        let start_time = Instant::now();

        // Hash and normalize all phone numbers
        let mut hashed_numbers = Vec::new();
        for phone_number in &request.phone_numbers {
            match normalize_and_hash(phone_number, &request.country_code) {
                Ok(hashed) => hashed_numbers.push((hashed, phone_number.clone())),
                Err(_) => return Err(PhoneNumberError::InvalidPhoneNumber("Invalid phone number".to_string())),
            }
        }

        // Perform batch lookup for hashed numbers
        let batch_results = parallel_batches(
            dynamodb_client,
            hashed_numbers.iter().map(|(h, _)| h.clone()).collect()
        ).await;

        let batch_results = match batch_results {
            Ok(result) => result,
            Err(err) => {
                log::error!("Raw DynamoDB batch read error: {:?}", err);
                return Err(PhoneNumberError::DynamoDBReadFailed(format!("Detailed error: {}", err)));
            }
        };
        // Construct matched response
        let matched: Vec<PhoneCheckResponse> = hashed_numbers
            .into_iter()
            .filter_map(|(hashed, number)| {
                batch_results.get(&hashed).map(|wallet_address| PhoneCheckResponse {
                    phone_number: number,
                    wallet_address: wallet_address.clone(),
                })
            })
            .collect();

        let duration = start_time.elapsed().as_secs_f64();
        emit_metric(cloudwatch_client, "CheckPhoneNumbers", duration, StandardUnit::Seconds).await;

        Ok(matched)
    }).await
}


#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use foxy_shared::models::phone::PhoneCheckRequest;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::services::cloudwatch_services::create_cloudwatch_client;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};
    use super::*;


    #[tokio::test]
    async fn test_batch_lookup_returns_data() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Debug)
            .try_init();

        dotenv::dotenv().ok();
        let cognito_client = get_dynamodb_client_with_assumed_role().await;

        let test_hashes = vec![
            "430da017dc398ae959fd6b28db09aeae7c7c409d8e27b8590cfce1060dd8a450".to_string(),
            "fd4cd5c2f68838cd0b9781dfeb825c6edd3417141b5d930ee418946c044a273e".to_string(),
        ];

        let result = parallel_batches(&cognito_client, test_hashes.clone()).await.unwrap();

        log::info!("Result: {:?}", result);

        for hash in test_hashes {
            assert!(result.contains_key(&hash), "Missing result for hash: {}", hash);
        }
    }

    #[tokio::test]
    async fn integration_test() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "112527246877271240195";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let cloudwatch_client = create_cloudwatch_client().await;

        let phone_numbers = vec![
            "+447533907498".to_string(),
            "+447593322921".to_string(),
            "+447593322922".to_string(),
            "+447593322923".to_string(),
        ];

        let request = PhoneCheckRequest {
            phone_numbers,
            country_code: "GB".to_string(),
        };

        let result = check_phone_numbers(&access_token, &request, &dynamodb_client, &cloudwatch_client).await;

        match result {
            Ok(responses) => {
                //log::info!("Matched: {:?}", responses);

                let numbers: Vec<String> = responses.iter().map(|r| r.phone_number.clone()).collect();
                assert!(numbers.contains(&"+447533907498".to_string()));
                assert!(numbers.contains(&"+447593322921".to_string()));

                for r in responses {
                    assert!(
                        r.wallet_address.starts_with("0x"),
                        "Expected wallet address, got: {}",
                        r.wallet_address
                    );
                }
            }
            Err(err) => panic!("check_phone_numbers failed: {:?}", err),
        }

        Ok(())
    }
}