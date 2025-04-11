use serde_json::Value;
use reqwest::{Client, Response};
use serde_json::json;
use crate::models::errors::GasEstimateError;
use crate::models::estimate_flags::EstimateFlags;
use crate::models::transactions::{GasEstimate, TokenType, TransactionEstimateRequest};
use crate::services::cloudwatch_services::OperationMetricTracker;
use crate::track_rpc_call;
use crate::utilities::config;
use crate::utilities::config::get_rpc_url;

pub async fn estimate_gas(request: &TransactionEstimateRequest) -> Result<GasEstimate, GasEstimateError> {
    fetch_gas_from_source(request, ||None).await
}

pub async fn fetch_gas_from_source(
    request: &TransactionEstimateRequest,
    fetch_gas_data: impl Fn() -> Option<GasEstimate>,
) -> Result<GasEstimate, GasEstimateError> {
    if let Some(gas_fees) = fetch_gas_data() {
        return Ok(gas_fees);
    }

    //This url is only for L1 pricing, which we can't get from infura
    let network_url = config::get_ethereum_url();

    fetch_gas_from_api(&request.sender_address,
                       &request.recipient_address,
                       request.transaction_value,
                       &request.token_type,
                       &network_url).await
}

pub fn estimate_calldata_length(token_type: TokenType) -> usize {
    match token_type {
        TokenType::ETH => 0,
        TokenType::USDC => 68,
    }
}

pub async fn fetch_gas_from_api(
    sender: &str,
    recipient: &str,
    amount_in_base_units: Option<u128>,
    token_type: &TokenType,
    eth_mainnet_url: &str,
) -> Result<GasEstimate, GasEstimateError> {
    let tracker = OperationMetricTracker::build("Gas").await;

    let optimism_rpc = get_rpc_url();
    let client = Client::new();

    // Parallel fetch for gas price + gas limit (L2)
    let gas_price_res = track_rpc_call!(
            tracker,
            "eth_gasPrice",
            client.post(&optimism_rpc)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_gasPrice",
                    "params": []
                }))
                .send()
        );

    let gas_limit_res = track_rpc_call!(
            tracker,
            "eth_estimateGas",
            client.post(&optimism_rpc)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_estimateGas",
                    "params": [{
                        "from": sender,
                        "to": recipient,
                        "value": format!("0x{:x}", amount_in_base_units.unwrap()),
                        "data": "0x"
                    }]
                }))
                .send()
        );

    // Separate fetch for L1 gas price (from Ethereum mainnet)
    let l1_price_res = track_rpc_call!(
                tracker,
                "l1_gas_price",
                client.post(eth_mainnet_url)
                    .json(&json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "eth_gasPrice",
                        "params": []
                    }))
                    .send()
            );

    let gas_price_json = validate_response("Gas Price", gas_price_res).await?;
    let gas_limit_json = validate_response("Gas Limit", gas_limit_res).await?;
    let l1_price_json = validate_response("L1 Gas Price", l1_price_res).await?;

    let mut estimate_flags = EstimateFlags::empty();
    let (gas_limit, gas_flag) = classify_and_maybe_return("Gas Limit", &gas_limit_json)?;
    estimate_flags |= gas_flag;

    let (gas_price, price_flag) = classify_and_maybe_return("Gas Price", &gas_price_json)?;
    estimate_flags |= price_flag;

    let (l1_gas_price, l1_gas_flag) = classify_and_maybe_return("L1 Gas Price", &l1_price_json)?;
    estimate_flags |= l1_gas_flag;

    // Apply fixed scalar in basis points
    const L1_SCALAR_BPS: u64 = 12000; // 1.2x
    let calldata_len = estimate_calldata_length(token_type.clone());
    let l1_gas_used = 4 * calldata_len as u64;
    let l1_fee = ((l1_gas_used as u128 * l1_gas_price as u128) * L1_SCALAR_BPS as u128) / 10_000;

    // Final fee summary -
    let priority_fee = 1_000u64; // 1000 wei (0.000001 gwei) is a good floor
    let max_fee_per_gas = gas_price + 10 * priority_fee; // generous buffer

    let network_fee = (gas_limit as u128) * (max_fee_per_gas as u128);

    tracker.track::<(), ()>(
        &Ok(()),
        Some(network_fee as f64),
    ).await;

    tracker.emit(
        "GasValue",
        l1_fee as f64,
        "None",
        &[("Type", "l1_fee")],
    ).await;

    tracker.emit(
        "GasValue",
        max_fee_per_gas as f64,
        "None",
        &[("Type", "max_fee_per_gas")],
    ).await;

    Ok(GasEstimate {
        status: estimate_flags,
        gas_limit,
        gas_price,
        l1_fee,
        max_fee_per_gas,
        max_priority_fee_per_gas: 150000,
        network_fee,
    })
}

pub fn classify_and_maybe_return(
    label: &str,
    json: &serde_json::Value,
) -> Result<(u64, EstimateFlags), GasEstimateError> {
    // Happy path: use the gas estimate result
    if let Some(_) = json.get("result") {
        let parsed = parse_json_hex(json, "result")?;
        return Ok((parsed, EstimateFlags::SUCCESS));
    }

    // Handle known RPC errors
    if let Some(error_obj) = json.get("error") {
        if let Some(message) = error_obj.get("message").and_then(|m| m.as_str()) {
            let flags = classify_estimate_error(message);

            match flags {
                EstimateFlags::INVALID_OPCODE |
                EstimateFlags::CONTRACT_REVERTED |
                EstimateFlags::RPC_AUTHENTICATION_FAILED |
                EstimateFlags::EXECUTION_REVERTED => {
                    return Err(GasEstimateError::ApiError(label.to_string(), message.to_string()));
                }
                _ => {
                    // Recoverable error – we still want to continue
                    return Ok((0, flags));
                }
            }
        }
    }

    // Completely unexpected: neither result nor error
    Err(GasEstimateError::IncompleteResponse(label.to_string()))
}


fn classify_estimate_error(message: &str) -> EstimateFlags {
    let msg = message.to_lowercase();
    let mut flags = EstimateFlags::empty();

    if msg.contains("insufficient funds")
        || msg.contains("balance")
        || msg.contains("transfer amount exceeds balance")
        || msg.contains("insufficient eth")
        || msg.contains("insufficient output amount")
    {
        flags |= EstimateFlags::INSUFFICIENT_FUNDS;
    }

    if msg.contains("execution reverted")
        && (msg.contains("allowance")
        || msg.contains("caller is not the owner")
        || msg.contains("only owner")
        || msg.contains("not authorized"))
    {
        flags |= EstimateFlags::CONTRACT_REVERTED;
    }

    if msg.contains("execution reverted") && msg.contains("invalid opcode") {
        flags |= EstimateFlags::INVALID_OPCODE;
    }

    if msg.contains("gas required exceeds allowance")
        || msg.contains("out of gas")
        || msg.contains("intrinsic gas too low")
    {
        flags |= EstimateFlags::GAS_LIMIT_TOO_LOW;
    }

    if msg.contains("nonce too low") || msg.contains("replacement transaction underpriced") {
        flags |= EstimateFlags::NONCE_ERROR;
    }

    if msg.contains("chain id mismatch")
        || msg.contains("unknown account")
        || msg.contains("signature")
    {
        flags |= EstimateFlags::SIGNATURE_INVALID;
    }

    if msg.contains("rate limit") || msg.contains("request rate") {
        flags |= EstimateFlags::RATE_LIMITED;
    }

    if msg.contains("quota") || msg.contains("exceeded") {
        flags |= EstimateFlags::QUOTA_EXCEEDED;
    }

    if msg.contains("missing project id") || msg.contains("authentication") {
        flags |= EstimateFlags::RPC_AUTHENTICATION_FAILED;
    }

    flags
}

pub async fn validate_response(
    name: &str,
    response: Result<Response, reqwest::Error>,
) -> Result<Value, GasEstimateError> {
    let resp = response.map_err(|e| {
        log::error!("[{}] HTTP request failed: {}", name, e);
        GasEstimateError::RequestError(name.to_string(), e.to_string())
    })?;

    let body: Value = resp.json().await.map_err(|e| {
        log::error!("[{}] Failed to parse JSON: {}", name, e);
        GasEstimateError::ParseError(name.to_string(), e.to_string())
    })?;

    // ✅ Always return the full JSON body, even if it has an "error" field
    Ok(body)
}

// Helper function to parse hex values from JSON response
fn parse_json_hex(json: &serde_json::Value, key: &str) -> Result<u64, GasEstimateError> {
    json.get(key)
        .and_then(|v| v.as_str())
        .and_then(|hex| u64::from_str_radix(&hex[2..], 16).ok())
        .ok_or_else(|| GasEstimateError::IncompleteResponse(format!("Missing or invalid {} field", key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transactions::TokenType;

    #[test]
    fn test_calldata_length_eth() {
        let len = estimate_calldata_length(TokenType::ETH);
        assert_eq!(len, 0, "ETH transfers should have 0 calldata bytes");
    }

    #[test]
    fn test_calldata_length_usdc() {
        let len = estimate_calldata_length(TokenType::USDC);
        assert_eq!(len, 68, "USDC (ERC-20) transfers should have 68 calldata bytes");
    }

    #[tokio::test]
    async fn test_transaction_estimate() {
        dotenv::dotenv().ok(); // Load .env with RPC URLs
        let _ = env_logger::builder().is_test(true).try_init();
        let ethereum_url = config::get_ethereum_url();

        let result = fetch_gas_from_api("0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC",
                                        "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7",
                                        Some(1_000_000_000_000_000_000_000_000_000u128),
                                        &TokenType::ETH,
                                        &ethereum_url).await;

        assert!(result.is_ok(), "Gas estimation failed: {:?}", result.err());

        let estimate = result.unwrap();
        println!("Estimate: {:?}", estimate);

        assert_eq!(estimate.gas_limit, 0, "Gas limit should be zero");
        assert!(estimate.gas_price > 0, "Gas price should be greater than zero");

        // ⚠️ Optional: if you're expecting L1 to be 0 for ETH txs
        assert_eq!(estimate.l1_fee, 0, "ETH tx should have 0 L1 fee");

        // ✅ Check for flag presence
        assert!(
            estimate.status.contains(EstimateFlags::INSUFFICIENT_FUNDS),
            "Expected INSUFFICIENT_FUNDS flag to be present"
        );

        // ✅ Check that it still includes SUCCESS if that's your logic
        assert!(
            estimate.status.contains(EstimateFlags::SUCCESS),
            "Expected SUCCESS flag to be present"
        );
    }
}