use ethers_core::types::Address;
use reqwest::Client;
use serde_json::json;
use std::str::FromStr;
use crate::utilities::config::get_rpc_url;
use once_cell::sync::Lazy;
use crate::models::errors::NonceError;

static SHARED_CLIENT: Lazy<Client> = Lazy::new(Client::new);

pub struct NonceManager {
    rpc_url: String,
    client: Client,
}

impl NonceManager {
    pub fn new() -> Result<Self, NonceError> {
        let rpc_url = get_rpc_url();
        Ok(Self {
            rpc_url,
            client: SHARED_CLIENT.clone(),
        })
    }

    pub async fn get_nonce(&self, address: &str) -> Result<u64, NonceError> {
        let parsed_address = Address::from_str(address)
            .map_err(|_| NonceError::InvalidAddress(address.to_string()))?;

        let payload = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionCount",
            "params": [format!("0x{:x}", parsed_address), "pending"],
            "id": 1
        });

        let res = self.client
            .post(&self.rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NonceError::HttpRequestError(e.to_string()))?;

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| NonceError::HttpRequestError(e.to_string()))?;

        let result = json.get("result").and_then(|v| v.as_str()).ok_or(NonceError::InvalidResponse)?;

        u64::from_str_radix(result.trim_start_matches("0x"), 16)
            .map_err(|_| NonceError::InvalidResponse)
    }
}
