use serde_json::Value;
use reqwest::Client;
use serde_json::json;
use foxy_shared::services::authentication::generate_tokens;
use dotenv::dotenv;
use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;
#[tokio::test]
async fn test_transaction_initiate_success() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv().is_ok();
    let api_url = "http://localhost:9000";
    let url = format!("{}/transactions/initiate", api_url);
    let client = Client::new();

    let test_user_id = "112527246877271240195";
    let cognito_client = get_cognito_client_with_assumed_role().await?;
    let token_result = generate_tokens(&cognito_client, &test_user_id)
        .await
        .expect("Failed to get test token");
    let access_token = token_result.access_token.expect("Access token missing");

    let valid_request = json!({
        "sender_address": "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923",
        "recipient_address": "0xe4d798c5b29021cdecda6c2019d3127af09208ca",
        "fiat_value": 5000,
        "fiat_currency_code": "GBP",
        "transaction_value": "1000000000000000", // 0.001 ETH
        "token_type": "ETH",
        "message": "Let's get coffee",
        "gas_estimate": {
            "status": "SUCCESS",
            "gas_limit": 21000,
            "gas_price": 1000000000,
            "l1_fee": "0",
            "max_fee_per_gas": 1000000000,
            "max_priority_fee_per_gas": 1000000000,
            "network_fee": "21000000000000"
        },
        "exchange_rate": 2300.0,
        "service_fee": "10000000000000",
        "network_fee": "21000000000000"
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&valid_request)
        .send()
        .await
        .expect("Failed to send request");

    let _status = response.status();

    //Test the response
    let unsigned_tx: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");
    println!("🚀 Transaction created: {}", unsigned_tx);    //Test the response
    assert_eq!(unsigned_tx.get("tx_type").unwrap().as_u64().unwrap(), 2);
    assert_eq!(
        unsigned_tx.get("to").unwrap().as_str().unwrap(),
        "0xe4d798c5b29021cdecda6c2019d3127af09208ca"
    );
    assert_eq!(
        unsigned_tx.get("amount_base_units").unwrap().as_str().unwrap(),
        "1000000000000000"
    );
    assert_eq!(
        unsigned_tx.get("gas_limit").unwrap().as_str().unwrap(),
        "21000"
    );
    assert_eq!(
        unsigned_tx.get("gas_price").unwrap().as_str().unwrap(),
        "1000000000"
    );
    assert_eq!(
        unsigned_tx.get("nonce").unwrap().as_str().unwrap(),
        "0"
    );
    assert_eq!(
        unsigned_tx.get("chain_id").unwrap().as_str().unwrap(),
        "11155420"
    );
    assert_eq!(
        unsigned_tx.get("token_type").unwrap().as_str().unwrap(),
        "ETH"
    );
    assert_eq!(
        unsigned_tx.get("token_decimals").unwrap().as_u64().unwrap(),
        18
    );


    Ok(())
}
