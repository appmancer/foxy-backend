use dotenv::dotenv;
use http::StatusCode;
use reqwest::Client;
use serde_json::json;
use foxy_shared::services::authentication::generate_tokens;
use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;
#[tokio::test]
async fn test_device_save() -> Result<(), Box<dyn std::error::Error>>{
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let url = format!("{}/phone/device", api_url);
    let client = Client::new();

    let test_user_id = "108298283161988749543"; //Jack
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let valid_request = json!({
        "device_fingerprint": "48cf2b26-337f-4fa1-adab-36c0e33b1485",
        "push_token": "abc123",
        "platform": "Android",
        "app_version": "0.1.0",
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&valid_request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED, "Expected 201 CREATED");

    Ok(())
}

#[tokio::test]
async fn test_device_parse_fail() -> Result<(), Box<dyn std::error::Error>>{
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let url = format!("{}/phone/device", api_url);
    let client = Client::new();

    let test_user_id = "108298283161988749543"; //Jack
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let valid_request = json!({
        "device_fingerprint": "48cf2b26-337f-4fa1-adab-36c0e33b1485",
        "platform": "Android",
        "app_version": "0.1.0",
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&valid_request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "Expected 400 FAILED");

    Ok(())
}