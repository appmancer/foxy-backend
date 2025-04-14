use std::sync::Arc;
use std::time::Instant;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::services::cognito_services::get_cognito_client;
use foxy_shared::models::transactions::{GasEstimate, TransactionBuilder, TransactionRequest, UnsignedTransaction};
use foxy_shared::models::errors::TransactionError;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::nonce_manager::NonceManager;
use foxy_shared::utilities::config::get_transaction_event_table;
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::database::client::get_dynamodb_client;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let mut transaction_request: TransactionRequest  = serde_json::from_value(body)
        .map_err(|_| TransactionError::InvalidRequest)?;

    // Convert gas_pricing → gas_estimate if needed
    if transaction_request.gas_estimate.is_none() {
        if let Some(gp) = &transaction_request.gas_pricing {
            transaction_request.gas_estimate = Some(GasEstimate::try_from(gp.clone())?);
        } else {
            log::error!("Missing gas_pricing and gas_estimate in request");
            return Err(TransactionError::InvalidRequest.into());
        }
    }

    let cloudwatch_client = create_cloudwatch_client().await;

    let cognito_client = get_cognito_client().await;
    let dynamo_db_client = get_dynamodb_client().await;
    log::info!("Starting transaction");

    match (token, transaction_request) {
        (Some(token), transaction_request) =>
                match handle_transaction_initiation(token,
                                                    transaction_request,
                                                    &cognito_client,
                                                    &dynamo_db_client,
                                                    &cloudwatch_client).await {
                    Ok(response) => success_response(response),
                    Err(err) => error_response(format!("{:?}", err)),
            },
        (None, _) => error_response("Missing authorization token"),
    }
}

/// Handles transaction validation and response generation.
async fn handle_transaction_initiation(
            token: &str,
            request: TransactionRequest,
            cognito_client: &CognitoClient,
            dynamo_db_client: &DynamoDbClient,
            cloudwatch_client: &CloudWatchClient)
    -> Result<UnsignedTransaction, TransactionError> {
    with_valid_user(token, |user_id| async move {
        log::info!("Initiating transaction for user: {}", user_id);
        let start_time = Instant::now();

        // Validate transaction request
        validate_transaction_request(&request)?;

        //Create a transaction builder from the request
        let mut builder = TransactionBuilder::try_from(request)?;
        builder.sender_user_id = user_id;

        // Get nonce for sender address
        let nonce_manager = NonceManager::new()?;
        builder.nonce = nonce_manager.get_nonce(&builder.sender_address).await?;
        log::info!("Got nonce: {:?}", builder.nonce);

        // Log metrics
        let elapsed_time = start_time.elapsed().as_millis() as f64;
        emit_metric(cloudwatch_client, "ValidationLatency", elapsed_time, StandardUnit::Milliseconds).await;
        emit_metric(cloudwatch_client, "ValidationSuccessCount", 1.0, StandardUnit::Count).await;

        match builder.build(&cognito_client, &dynamo_db_client).await {
            Ok(mut transaction) => {
                let manager = TransactionEventManager::new(
                                                        Arc::new(dynamo_db_client.clone()),
                                                        get_transaction_event_table(),
                                                    );
                manager.persist_initial_event(&mut transaction).await?;

                //We need to return an unsigned transactions
                let unsigned_tx = UnsignedTransaction::from(&transaction);
                Ok(unsigned_tx)
            }
            Err(err) => {
                log::error!("Transaction creation failed: {:?}", err);
                Err(TransactionError::InvalidRequest)
            }
        }
    }).await
}

fn is_valid_address(address: &str) -> bool {
    address.len() == 42 && address.starts_with("0x")
}

fn validate_transaction_request(request: &TransactionRequest) -> Result<(), TransactionError> {
    if request.fiat_value == 0 {
        return Err(TransactionError::InvalidAmount);
    }

    if !is_valid_address(&request.sender_address) || !is_valid_address(&request.recipient_address) {
        return Err(TransactionError::InvalidAddress);
    }

    if request.sender_address == request.recipient_address {
        return Err(TransactionError::SameSenderReceiver);
    }

    if request.exchange_rate <= 0.0 {
        return Err(TransactionError::InvalidExchangeRate);
    }

    if request.transaction_value == 0 {
        return Err(TransactionError::InvalidTransactionValue);
    }

    if request.network_fee == 0 {
        return Err(TransactionError::InvalidNetworkFee);
    }

    if request.service_fee == 0 {
        return Err(TransactionError::InvalidServiceFee);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use foxy_shared::models::transactions::{TransactionRequest, TransactionBuilder, Network, PriorityLevel, TransactionStatus, TokenType, GasPricing};
    use std::convert::TryFrom;
    use dotenv::dotenv;
    use foxy_shared::models::estimate_flags::EstimateFlags;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};

    #[tokio::test]
    async fn test_transaction_builder_from_request() {
        use foxy_shared::models::transactions::{TransactionRequest, TransactionBuilder, TokenType};
        use std::convert::TryFrom;

        let request = TransactionRequest {
            sender_address: "0x1111111111111111111111111111111111111111".to_string(),
            recipient_address: "0x2222222222222222222222222222222222222222".to_string(),
            fiat_value: 5000, // e.g., £50.00
            fiat_currency_code: "GBP".to_string(),
            transaction_value: 1_000_000_000_000_000_000u128, // 1 ETH in wei
            token_type: TokenType::ETH,
            message: Some("Here’s £50".to_string()),
            exchange_rate: 2000.0,
            service_fee: 1000,
            network_fee: 21010941000,
            gas_pricing: Some(GasPricing{
                estimated_gas: "21000".to_string(),
                gas_price: "1000521". to_string(),
                max_fee_per_gas: "1200625".to_string(),
                max_priority_fee_per_gas: "0".to_string(),
            }),
            gas_estimate: None,
        };

        let builder_result = TransactionBuilder::try_from(request);

        match builder_result {
            Ok(builder) => {
                assert_eq!(builder.fiat_value, 5000);
                assert_eq!(builder.fiat_currency_code, "GBP");
                assert_eq!(builder.sender_address, "0x1111111111111111111111111111111111111111");
                assert_eq!(builder.recipient_address, "0x2222222222222222222222222222222222222222");
                assert_eq!(builder.transaction_value, 1_000_000_000_000_000_000u128);
                assert_eq!(builder.rate, 2000.0);
                assert_eq!(builder.gas_estimate.gas_limit, 21000);
                assert_eq!(builder.gas_estimate.network_fee, 21010941000);
                assert_eq!(builder.message.unwrap(), "Here’s £50");
            },
            Err(e) => panic!("❌ Failed to convert TransactionRequest to TransactionBuilder: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_transaction_builder_builds_transaction_successfully() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();

        // Manually construct a TransactionBuilder to simulate intermediate step
        let builder = TransactionBuilder {
            sender_address: "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923".to_string(),
            sender_user_id: "112527246877271240195".to_string(), // Assume mapped from Cognito
            recipient_address: "0xe4d798c5b29021cdecda6c2019d3127af09208ca".to_string(),
            fiat_value: 1000,
            fiat_currency_code: "GBP".to_string(),
            transaction_value: 1_000_000_000_000_000_000u128, // 1 ETH in wei
            message: Some("Happy birthday".to_string()),
            rate: 100.0,
            network: Network::OptimismSepolia,
            token_type: TokenType::ETH,
            priority_level: PriorityLevel::Standard,
            nonce: 1,
            gas_estimate: GasEstimate {
                status: EstimateFlags::SUCCESS,
                gas_limit: 21000,
                gas_price: 100_000_000_0,
                l1_fee: 0,
                max_fee_per_gas: 150_000_000_0,
                max_priority_fee_per_gas: 0,
                network_fee: 21000 * 100_000_000_0,
            },
            exchange_rate: 1200.0,
            network_fee: 0,
            service_fee: 20,
        };

        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;

        let transaction = builder.build(&cognito_client, &dynamo_db_client).await?;
        println!("✅ Built transaction: {:?}", transaction);

        assert_eq!(transaction.sender_address, "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923");
        assert_eq!(transaction.recipient_address, "0xe4d798c5b29021cdecda6c2019d3127af09208ca");
        assert_eq!(transaction.transaction_value, 1000000000000000000u128);
        assert_eq!(transaction.token_type, TokenType::ETH);
        assert_eq!(transaction.status, TransactionStatus::Created);
        assert_eq!(transaction.network_fee, 0);
        assert_eq!(transaction.service_fee, 20);
        assert_eq!(transaction.total_fees, 20);
        assert_eq!(transaction.fiat_value, 1000);
        assert_eq!(transaction.fiat_currency, "GBP");
        assert_eq!(transaction.priority_level, PriorityLevel::Standard);
        assert_eq!(transaction.network, Network::OptimismSepolia);
        assert!(transaction.transaction_id.len() > 0);
        assert!(transaction.metadata.message.contains(""));
        assert_eq!(transaction.metadata.display_currency, "GBP");
        assert_eq!(transaction.metadata.expected_currency_amount, 1000);
        assert_eq!(transaction.metadata.from.wallet, "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923");
        assert_eq!(transaction.metadata.to.wallet, "0xe4d798c5b29021cdecda6c2019d3127af09208ca");
        assert!(transaction.exchange_rate.is_some());
        assert!(transaction.gas_price.is_some());
        assert!(transaction.gas_limit.is_some());
        assert!(transaction.nonce.is_some());
        assert!(transaction.max_fee_per_gas.is_some());
        assert!(transaction.max_priority_fee_per_gas.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_request_builds_transaction_successfully() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();

        // Setup dummy transaction request
        let request = TransactionRequest {
            sender_address: "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923".to_string(),
            recipient_address: "0xe4d798c5b29021cdecda6c2019d3127af09208ca".to_string(),
            fiat_value: 5000, // e.g., £50.00
            fiat_currency_code: "GBP".to_string(),
            transaction_value: 1_000_000_000_000_000_000u128, // 1 ETH in wei
            token_type: TokenType::ETH,
            message: Some("Here’s £50".to_string()),
            exchange_rate: 2000.0,
            service_fee: 1000,
            network_fee: 500,
            gas_pricing: Some(GasPricing{
                estimated_gas: "21000".to_string(),
                gas_price: "1000521". to_string(),
                max_fee_per_gas: "1200625".to_string(),
                max_priority_fee_per_gas: "0".to_string(),
            }),
            gas_estimate: None,
        };

        let builder = TransactionBuilder::try_from(request);
        assert!(builder.is_ok());
        let builder = builder.unwrap();

        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;

        let transaction = builder.build(&cognito_client, &dynamo_db_client).await?;
        println!("✅ Built transaction: {:?}", transaction);

        assert_eq!(transaction.sender_address, "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923");
        assert_eq!(transaction.recipient_address, "0xe4d798c5b29021cdecda6c2019d3127af09208ca");
        assert_eq!(transaction.transaction_value, 1000000000000000000);
        assert_eq!(transaction.token_type, TokenType::ETH);
        assert_eq!(transaction.status, TransactionStatus::Created);
        assert_eq!(transaction.network_fee, 500);
        assert_eq!(transaction.service_fee, 1000);
        assert_eq!(transaction.total_fees, 1500);
        assert_eq!(transaction.fiat_value, 5000);
        assert_eq!(transaction.fiat_currency, "GBP");
        assert_eq!(transaction.priority_level, PriorityLevel::Standard);
        assert_eq!(transaction.network, Network::OptimismSepolia);
        assert!(transaction.transaction_id.len() > 0);
        assert!(transaction.metadata.message.contains("Here’s £50"));
        assert_eq!(transaction.metadata.display_currency, "GBP");
        assert_eq!(transaction.metadata.expected_currency_amount, 5000);
        assert_eq!(transaction.metadata.from.wallet, "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923");
        assert_eq!(transaction.metadata.to.wallet, "0xe4d798c5b29021cdecda6c2019d3127af09208ca");
        assert!(transaction.exchange_rate.is_some());
        assert!(transaction.gas_price.is_some());
        assert!(transaction.gas_limit.is_some());
        assert!(transaction.nonce.is_some());
        assert!(transaction.max_fee_per_gas.is_some());
        assert!(transaction.max_priority_fee_per_gas.is_some());
        Ok(())
    }


    #[tokio::test]
    async fn test_handler() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();

        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;
        let cloudwatch_client = create_cloudwatch_client().await;
        let test_user_id = "112527246877271240195";

        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        // Setup dummy transaction request
        let request = TransactionRequest {
            sender_address: "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923".to_string(),
            recipient_address: "0xe4d798c5b29021cdecda6c2019d3127af09208ca".to_string(),
            fiat_value: 5000, // e.g., £50.00
            fiat_currency_code: "GBP".to_string(),
            transaction_value: 1_000_000_000_000_000u128, // 0.001 ETH in wei
            token_type: TokenType::ETH,
            message: Some("Here’s £50".to_string()),
            exchange_rate: 2000.0,
            service_fee: 1000,
            network_fee: 500,
            gas_pricing: Some(GasPricing{
                estimated_gas: "21000".to_string(),
                gas_price: "1000521". to_string(),
                max_fee_per_gas: "1200625".to_string(),
                max_priority_fee_per_gas: "0".to_string(),
            }),
            gas_estimate: None,
        };

        match handle_transaction_initiation(access_token.as_str(),
                                            request,
                                            &cognito_client,
                                            &dynamo_db_client,
                                            &cloudwatch_client).await {
            Ok(response) => {
                println!("Transaction success: {:?}", response);
                assert_eq!(response.token_type, TokenType::ETH); // or USDC
                assert_eq!(response.tx_type, 2);
                assert_eq!(response.to, "0xe4d798c5b29021cdecda6c2019d3127af09208ca".to_string());
                assert_eq!(response.amount_base_units, "1000000000000000".to_string());
                assert!(response.nonce.len() > 0);
                assert_eq!(response.gas_limit, "21000".to_string());
                assert_eq!(response.gas_price, "1000521".to_string());
                assert_eq!(response.chain_id, "11155420".to_string());
                assert_eq!(response.token_decimals, 18);
            },
            Err(err) => panic!("{}", err),
        }

        Ok(())
    }

}
