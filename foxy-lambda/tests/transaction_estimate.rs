use reqwest::Client;
use serde_json::json;
use foxy_shared::services::authentication::generate_tokens;
use dotenv::dotenv;
use http::StatusCode;
use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;
#[tokio::test]
async fn test_transaction_estimate_success() -> Result<(), Box<dyn std::error::Error>>{
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/estimate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let valid_request = json!({
        "sender_address": "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC",
        "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
        "token_type": "ETH",
        "fiat_value": 500, //500 pence
        "fiat_currency": "GBP",
        /* no WEI, let the esimator calculate it */
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&valid_request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("Invalid JSON response");

    // Check presence of top-level fields
    assert_eq!(body["token_type"], "ETH");
    assert_eq!(body["fiat_currency"], "GBP");
    assert_eq!(body["fiat_amount_minor"], 500);

    // Must contain address and estimated amounts
    assert!(
        body["recipient_address"].as_str().is_some(),
        "recipient_address must be present"
    );
    assert!(
        body["eth_amount"].as_str().unwrap_or("").parse::<f64>().unwrap_or(0.0) > 0.0,
        "eth_amount should be a positive float"
    );
    assert!(
        body["wei_amount"].as_str().unwrap_or("").parse::<u128>().unwrap_or(0) > 0,
        "wei_amount should be a positive integer"
    );

    // Fees must be included and non-empty
    let fees = &body["fees"];
    for field in &[
        "service_fee_wei",
        "service_fee_eth",
        "network_fee_wei",
        "network_fee_eth",
        "total_fee_wei",
        "total_fee_eth",
        "max_priority_fee_per_gas",
    ] {
        assert!(
            fees[field].as_str().unwrap_or("").parse::<f64>().unwrap_or(0.0) >= 0.0,
            "{} should be a non-negative number",
            field
        );
    }

    // Gas fields must be present and numeric
    let gas = &body["gas"];
    for field in &[
        "estimated_gas",
        "gas_price",
        "max_fee_per_gas",
    ] {
        assert!(
            gas[field].as_str().unwrap_or("").parse::<u64>().unwrap_or(0) > 0,
            "{} should be a positive number",
            field
        );
    }

    // Exchange rate must be present and > 0
    let exchange_rate: f64 = body["exchange_rate"].as_f64().unwrap_or(0.0);
    assert!(
        exchange_rate > 0.0,
        "exchange_rate should be a positive float"
    );

    // Exchange rate expiry must be in the future
    let expiry_str = body["exchange_rate_expires_at"]
        .as_str()
        .expect("exchange_rate_expires_at should be a string");
    let expiry = chrono::DateTime::parse_from_rfc3339(expiry_str)
        .expect("exchange_rate_expires_at should be a valid datetime");
    assert!(
        expiry > chrono::Utc::now(),
        "exchange_rate_expires_at should be in the future"
    );

    // Status must include "SUCCESS"
    let status_array = body["status"]
        .as_array()
        .expect("status must be an array of strings");
    let status_strings: Vec<String> = status_array
        .iter()
        .filter_map(|s| s.as_str().map(|v| v.to_string()))
        .collect();

    assert!(
        status_strings.contains(&"SUCCESS".to_string()),
        "EstimateFlags should include SUCCESS: got {:?}",
        status_strings
    );

    Ok(())
}

#[tokio::test]
async fn test_transaction_estimate_invalid_recipient_address() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/estimate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
    let access_token = token_result.access_token.expect("Access token missing");

    let invalid_request = json!({
        "sender_address": "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC",
        "recipient_address": "0xINVALIDADDRESS123456789",  // <- invalid address
        "token_type": "ETH",
        "fiat_amount": 500,
        "fiat_currency": "GBP",
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&invalid_request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = response.json().await.expect("Invalid JSON response");
    assert!(body["status"].as_array().unwrap().contains(&json!("WALLET_NOT_FOUND")));
    Ok(())
}

#[tokio::test]
async fn test_transaction_estimate_insufficient_funds() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/estimate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
    let access_token = token_result.access_token.expect("Access token missing");

    let broke_address = "0x0000000000000000000000000000000000000001";
    let request = json!({
        "sender_address": broke_address,
        "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
        "token_type": "ETH",
        "fiat_amount": 500_000,  // Large fiat amount to exceed 0 ETH balance
        "fiat_currency": "GBP",
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("Invalid JSON response");
    assert!(
        body["status"].as_array().unwrap().contains(&json!("INSUFFICIENT_FUNDS")),
        "Expected INSUFFICIENT_FUNDS in status"
    );
    Ok(())
}

#[tokio::test]
async fn test_transaction_estimate_invalid_token_type() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/estimate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
    let access_token = token_result.access_token.expect("Access token missing");

    let request = json!({
        "sender_address": "0x0000000000000000000000000000000000000001",
        "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
        "token_type": "DOGE",  // Unsupported token
        "fiat_amount": 500,
        "fiat_currency": "GBP"
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.expect("Invalid JSON response");
    println!("Response: {:?}", body);
    Ok(())
}

#[tokio::test]
async fn test_transaction_estimate_unsupported_fiat_currency() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/estimate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
    let access_token = token_result.access_token.expect("Access token missing");

    let request = json!({
        "sender_address": "0x0000000000000000000000000000000000000001",
        "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
        "token_type": "ETH",
        "fiat_amount": 500,
        "fiat_currency": "XYZ"  // Unsupported currency
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("Invalid JSON response");
    println!("Response: {:?}", body);
    Ok(())
}
