use dotenv::dotenv;
use http::StatusCode;
use reqwest::Client;
use serde_json::json;
use foxy_shared::models::transactions::TransactionHistoryItem;
use foxy_shared::services::authentication::generate_tokens;
use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;

#[tokio::test]
async fn test_get_recent_transactions() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let url = format!("{}/transactions/recent", api_url);
    let client = Client::new();

    let test_user_id = "108298283161988749543"; // replace with a real test user
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK, "Expected 200 OK");

    let body = response.text().await.expect("Failed to read response body");
    let transactions: Vec<TransactionHistoryItem> = serde_json::from_str(&body)
        .expect("Failed to deserialize response");

    assert!(!transactions.is_empty(), "Expected at least one transaction");

    Ok(())
}

#[tokio::test]
async fn test_post_recent_transactions_with_limit() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let url = format!("{}/transactions/recent", api_url);
    let client = Client::new();

    let test_user_id = "108298283161988749543"; // replace with a valid test user
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let paging_body = json!({
        "limit": 5
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&paging_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK, "Expected 200 OK");

    let body = response.text().await.expect("Failed to read response body");
    let transactions: Vec<TransactionHistoryItem> = serde_json::from_str(&body)
        .expect("Failed to deserialize response");

    assert!(
        transactions.len() <= 5,
        "Expected no more than 5 transactions, got {}",
        transactions.len()
    );

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_bundle_id() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let bundle_id = "1234";

    let url = format!("{}/transactions/{}", api_url, bundle_id);
    let client = Client::new();

    let test_user_id = "108298283161988749543"; // replace with a test user that owns this bundle
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK, "Expected 200 OK");

    let body = response.text().await.expect("Failed to read response body");
    let transaction: TransactionHistoryItem = serde_json::from_str(&body)
        .expect("Failed to deserialize response");

    assert_eq!(transaction.bundle_id, bundle_id, "Returned wrong bundle");

    Ok(())
}


#[tokio::test]
async fn test_get_transaction_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000/lambda-url/foxy-lambda";
    let bundle_id = "bundle-does-not-exist";
    let url = format!("{}/transactions/{}", api_url, bundle_id);

    let client = Client::new();

    let test_user_id = "108298283161988749543"; // test user
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 Not Found"
    );

    let message = response.text().await.unwrap_or_default();
    assert!(
        message.contains("No transaction found"),
        "Unexpected error message: {}",
        message
    );

    Ok(())
}