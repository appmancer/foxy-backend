use anyhow::Result;
use reqwest::{Client, Response};
use serde_json::json;
use alloy_primitives::U256;
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromStr, ToPrimitive};
use crate::models::errors::WalletError;
use crate::utilities::config;

// Helper function to parse hex values from JSON response
fn parse_json_hex(json: &serde_json::Value, key: &str) -> std::result::Result<U256, WalletError> {
    json.get(key)
        .and_then(|v| v.as_str())
        .and_then(|hex| U256::from_str_radix(&hex[2..], 16).ok())
        .ok_or_else(|| WalletError::IncompleteResponse(format!("Missing or invalid {} field", key)))
}

async fn validate_response(endpoint: &str, response: std::result::Result<Response, reqwest::Error>) -> std::result::Result<serde_json::Value, WalletError> {
    match response {
        Ok(resp) => {
            let body = resp.text().await.map_err(|e| WalletError::Network(format!("Failed to read {} response: {:?}", endpoint, e)))?;
            log::debug!("{} Response: {}", endpoint, body);

            serde_json::from_str(&body).map_err(|e| WalletError::InvalidResponse(format!("{} JSON parse error: {:?}", endpoint, e)))
        }
        Err(e) => {
            log::error!("{} request failed: {:?}", endpoint, e);

            Err(WalletError::Network(format!("{} request failed: {:?}", endpoint, e)))
        }
    }
}

pub fn format_wei_to_eth_string(wei: U256, precision: usize) -> String {
    let wei_str = wei.to_string(); // e.g., "13816614144794697"
    let wei_decimal = Decimal::from_str(&wei_str).unwrap_or(Decimal::ZERO);
    let eth = wei_decimal / Decimal::from(1_000_000_000_000_000_000u128); // 1e18
    format!("{:.*}", precision, eth)
}

pub fn format_wei_to_eth_f64(wei: U256) -> f64 {
    let wei_str = wei.to_string(); // e.g., "13816614144794697"
    let wei_decimal = Decimal::from_str(&wei_str).unwrap_or(Decimal::ZERO);
    let eth = wei_decimal / Decimal::from(1_000_000_000_000_000_000u128); // 1e18
    Decimal::to_f64(&eth).unwrap()
}

pub async fn get_wallet_balance(wallet_address: &str) -> Result<U256, WalletError>
{
    let client = Client::new();
    let url = config::get_rpc_url();

    fetch_balance(&client, wallet_address, &url).await
}

async fn fetch_balance(client: &Client, wallet_address: &str, rpc_url: &str) -> Result<U256, WalletError> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_getBalance",
        "params": [wallet_address, "latest"],
        "id": 1
    });

    let get_balance = client.post(rpc_url)
        .json(&payload)
        .send()
        .await;

    // Validate responses
    let balance = validate_response("Get Balance", get_balance).await?;
    let wei = parse_json_hex(&balance, "result")?;

    Ok(wei)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_format_wei_to_eth_string() {
        // 1 ETH in Wei = 1_000_000_000_000_000_000
        let wei = U256::from_str("1000000000000000000").unwrap();
        let eth = format_wei_to_eth_string(wei, 6);
        assert_eq!(eth, "1.000000");

        // 0.1 ETH
        let wei = U256::from_str("100000000000000000").unwrap();
        let eth = format_wei_to_eth_string(wei, 6);
        assert_eq!(eth, "0.100000");

        // 0 ETH
        let wei = U256::from(0);
        let eth = format_wei_to_eth_string(wei, 6);
        assert_eq!(eth, "0.000000");

        // Small amount: 12345 Wei
        let wei = U256::from(12345);
        let eth = format_wei_to_eth_string(wei, 18);
        assert_eq!(eth, "0.000000000000012345");
    }
    #[tokio::test]
    async fn integration_test()
    {
        dotenv::dotenv().ok();
        let _ = tracing_subscriber::fmt::try_init();
        let client = Client::new();
        let url = config::get_rpc_url();
        let wallet_address = "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8";

        let wei = fetch_balance(&client, &wallet_address, &url);

        let balance = wei.await.unwrap();
        log::error!("balance: {}", balance);
        log::debug!("finish");
        /*
        match wei.await {
            Ok(balance) => {
                let debugme = balance;
                assert!(true)
            },
            Err(_) => assert!(false)
        }*/
    }
}