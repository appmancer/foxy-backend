use std::sync::Arc;
use std::time::Instant;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::StandardUnit;
use http::Response;
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::services::cognito_services::get_cognito_client;
use foxy_shared::models::transactions::{GasEstimate, TransactionBundle, TransactionRequest, UnsignedTransaction};
use foxy_shared::models::errors::TransactionError;
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::config::get_transaction_event_table;
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::database::client::get_dynamodb_client;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use crate::models::transactions::UnsignedTransactionPair;

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let mut transaction_request: TransactionRequest  = serde_json::from_value(body)
        .map_err(|_| TransactionError::InvalidRequest)?;
    tracing::info!("📬 Received transaction request: {:?}", transaction_request);

    // Convert gas_pricing → gas_estimate if needed
    if transaction_request.gas_estimate.is_none() {
        if let Some(gp) = &transaction_request.gas_pricing {
            transaction_request.gas_estimate = Some(GasEstimate::try_from(gp.clone())?);
            tracing::info!("📬 With populated gas estimate: {:?}", transaction_request);
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
    -> Result<UnsignedTransactionPair, TransactionError> {
    with_valid_user(token, |user_id| async move {
        log::info!("Initiating transaction for user: {}", user_id);
        let start_time = Instant::now();

        // Validate transaction request
        validate_transaction_request(&request)?;

        match TransactionBundle::from_request(user_id, request, cognito_client,dynamo_db_client).await {
            Ok(bundle) => {
                let manager = TransactionEventManager::new(
                                                        Arc::new(dynamo_db_client.clone()),
                                                        get_transaction_event_table(),
                                                    );
                manager.persist_initial_event(&bundle).await?;

                //We need to return unsigned transactions
                let unsigned_fee_tx = UnsignedTransaction::from(&bundle.fee_tx);
                let unsigned_main_tx = UnsignedTransaction::from(&bundle.main_tx);
                let unsigned_pair = UnsignedTransactionPair{
                    bundle_id: bundle.bundle_id,
                    fee: unsigned_fee_tx,
                    main: unsigned_main_tx,
                };

                // Log metrics
                let elapsed_time = start_time.elapsed().as_millis() as f64;
                emit_metric(cloudwatch_client, "ValidationLatency", elapsed_time, StandardUnit::Milliseconds).await;
                emit_metric(cloudwatch_client, "ValidationSuccessCount", 1.0, StandardUnit::Count).await;

                Ok(unsigned_pair)
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
    
    let gas = match &request.gas_estimate {
        Some(ge) => Some((ge.gas_limit, ge.max_fee_per_gas, ge.max_priority_fee_per_gas)),
        None => match &request.gas_pricing {
            Some(gp) => {
                let gas_limit = gp.estimated_gas.parse::<u64>().unwrap_or_default();
                let max_fee = gp.max_fee_per_gas.parse::<u64>().unwrap_or_default();
                let priority_fee = gp.max_priority_fee_per_gas.parse::<u64>().unwrap_or_default();
                Some((gas_limit, max_fee, priority_fee))
            },
            None => None,
        }
    };

    if let Some((gas_limit, max_fee, priority_fee)) = gas {
        if gas_limit < 21000 {
            return Err(TransactionError::MissingGasEstimate);
        }
        if max_fee == 0 {
            return Err(TransactionError::MissingGasEstimate);
        }
        if priority_fee == 0 {
            return Err(TransactionError::MissingGasEstimate);
        }
    } else {
        log::error!("❌ Missing both gas_estimate and gas_pricing");
        return Err(TransactionError::MissingGasEstimate);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use foxy_shared::models::transactions::{TransactionRequest, TokenType, GasPricing};
    use foxy_shared::models::user_device::UserDevice;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::config;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role, init_tracing};

    #[tokio::test]
    async fn test_handler() -> Result<(), Box<dyn std::error::Error>> {
        config::init();
        init_tracing();

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
            sender_address: "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0".to_string(),
            recipient_address: "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8".to_string(),
            fiat_value: 5000, // e.g., £50.00
            fiat_currency_code: "GBP".to_string(),
            transaction_value: 1_000_000_000_000_000u128, // 0.001 ETH in wei
            token_type: TokenType::ETH,
            message: Some("Here’s £50".to_string()),
            exchange_rate: 2000.0,
            service_fee: 1000,
            gas_pricing: Some(GasPricing{
                estimated_gas: "21000".to_string(),
                gas_price: "1000521". to_string(),
                max_fee_per_gas: "1200625".to_string(),
                max_priority_fee_per_gas: "0".to_string(),
            }),
            gas_estimate: None,
            service_fee_minor: 0,
            user_device: UserDevice::new("0eacf2aa-e788-4b54-bc1c-a95a05fc7d62".to_string(),
            "f30M3RyRSpKlDY7lbJBBKu:APA91bGH7m_zXvyYsCHdE5L7DDaT4ObWIe9y_5d3JKANJiM0zC6BJYcrTn1h9cfcaFgpK_hg2Sc32V951WQbP_kuv6ZwjITkhORb7G2pzx1RvbSsVyiu5eI".to_string(),
            "Android".to_string(), "0.1.0".to_string()),
        };

        match handle_transaction_initiation(access_token.as_str(),
                                            request,
                                            &cognito_client,
                                            &dynamo_db_client,
                                            &cloudwatch_client).await {
            Ok(response) => {
                println!("Transaction success: {:?}", response);
            }
            Err(err) => {
                eprintln!("Transaction initiation failed: {:?}", err);
                panic!("Test failed due to transaction initiation error.");
            }
        }

        Ok(())
    }

}
