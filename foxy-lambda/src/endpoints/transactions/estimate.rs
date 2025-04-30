use std::str::FromStr;
use ethers_core::types::Address;
use chrono::Utc;
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::models::errors::TransactionError;
use foxy_shared::utilities::{fees, gas};
use foxy_shared::models::transactions::{FeeBreakdown, GasPricing, TransactionEstimateRequest, TransactionEstimateResponse};
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, OperationMetricTracker};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use http::{Response, StatusCode};
use lambda_http::{Body, Request};
use serde_json::Value;
use foxy_shared::models::estimate_flags::EstimateFlags;
use foxy_shared::track_ok;
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::exchange::ExchangeRateManager;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, response_with_code};

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);

    let request: Result<TransactionEstimateRequest, TransactionError> = serde_json::from_value(body)
        .map_err(|e| {
            log::error!("Deserialization error: {:?}", e); // Log the actual error
            TransactionError::InvalidRequest
        });

    match &request{
        Ok(request) =>
            log::info!("Request is valid: {:?}", &request),
        Err(err) =>
            log::info!("Request is invalid: {:?}", err)
    };

    let cloudwatch_client = create_cloudwatch_client().await;
    let dynamodb_client = get_dynamodb_client().await;

    match (token, &request) {
        (Some(token), Ok(_)) => {
            match estimate_transaction(token, request.unwrap(), &dynamodb_client, &cloudwatch_client).await {
                Ok(response) => {
                    let code = status_code_for_estimate(response.status);
                    response_with_code(response, code)
                }
                Err(err) => error_response(format!("{:?}", err)),
            }
        }
        (Some(_), Err(err)) => error_response(format!("{:?}", err)),
        (None, Err(err)) => error_response(format!("{:?}", err)),
        (None, Ok(_)) => error_response("Missing authorization token"),
    }
}
fn status_code_for_estimate(flags: EstimateFlags) -> StatusCode {
    if flags.contains(EstimateFlags::INTERNAL_ERROR)
        || flags.contains(EstimateFlags::CONTRACT_REVERTED)
        || flags.contains(EstimateFlags::EXECUTION_REVERTED) {
            StatusCode::INTERNAL_SERVER_ERROR
    } else if flags.contains(EstimateFlags::RATE_LIMITED) {
        StatusCode::TOO_MANY_REQUESTS
    } else if flags.contains(EstimateFlags::SIGNATURE_INVALID) {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    }
}

async fn estimate_transaction(token: &str,
                                  request: TransactionEstimateRequest,
                                  dynamodb_client: &DynamoDbClient,
                                  cloudwatch_client: &CloudWatchClient)
                                  -> Result<TransactionEstimateResponse, TransactionError> {

    with_valid_user(token, |_| async move {
        let tracker = OperationMetricTracker::new(cloudwatch_client.clone(), "Estimate");
        track_ok!(tracker, async {
            if let Some(response) = early_exit_if_wallets_invalid(&request) {
                return Ok(response);
            }

            let mut status = EstimateFlags::empty();
            let exchange_rate;

            let exchange = ExchangeRateManager::new();

            //We have to get the exchange rate before we can price
            match exchange.get_latest_rate(&request.fiat_currency, &request.token_type).await{
                Ok(rate) => {
                    exchange_rate = rate;
                }
                Err(e) => {
                    //We should just bail now, no other figures can be calculated. Gotos are fine apparently.
                    let mut response = TransactionEstimateResponse::default();
                    response.token_type = request.token_type;
                    response.fiat_amount_minor = request.fiat_value;
                    response.fiat_currency = request.fiat_currency.clone();
                    response.status = EstimateFlags::SUCCESS | EstimateFlags::EXCHANGE_RATE_UNAVAILABLE;
                    response.message = Some(format!("Unable to fetch exchange rate: {}", e));
                    return Ok(response);
                }
            }
            /* The correct formula should be:
                estimated_wei=(fiat_amount×10¹⁸/exchange_rate*100)

                This ensures that fiat minor units (e.g., 1000 = £10.00) correctly map to WEI (10¹⁸ per ETH).
                The exchange rate from the exchange needs to be converted into minor units,
             */
            let mut request = request.clone();
            
            let pounds = (request.fiat_value as f64) / 100.0;
            let eth_amount = pounds / exchange_rate;
            let estimated_wei = (eth_amount * 1e18).floor() as u128;
            
            request.transaction_value = Some(estimated_wei);

            let gas_estimate = match gas::estimate_gas(&request).await {
                Ok(estimate) => {
                    status |= estimate.status;
                    estimate
                }
                Err(e) => {
                    status.insert(EstimateFlags::INTERNAL_ERROR);
                    return Err(e.into());
                }
            };

            //Important - the service fee is a % of the crypto value, not the fiat
            let service_fee = match request.transaction_value {
                Some(amount_wei) => {
                    match fees::calculate_service_fee(dynamodb_client, amount_wei).await {
                        Ok(fee) => fee,
                        Err(_) => {
                            status.insert(EstimateFlags::SERVICE_FEE_UNAVAILABLE);
                            0
                        }
                    }
                }
                None => {
                    status.insert(EstimateFlags::SERVICE_FEE_UNAVAILABLE);
                    0
                }
            };

            let total_fee = gas_estimate.network_fee + service_fee as u128;
            let exchange_rate_expires_at = Utc::now() + chrono::Duration::seconds(60);

            status = infer_estimate_success(status);

            Ok(TransactionEstimateResponse {
                token_type: request.token_type,
                fiat_amount_minor: request.fiat_value,
                fiat_currency: request.fiat_currency.clone(),
                eth_amount: format!("{:.8}", wei_to_eth(estimated_wei)),  // helper function
                wei_amount: estimated_wei.to_string(),

                fees: FeeBreakdown {
                    service_fee_wei: service_fee.to_string(),
                    service_fee_eth: format!("{:.8}", wei_to_eth(service_fee as u128)),
                    network_fee_wei: gas_estimate.network_fee.to_string(),
                    network_fee_eth: format!("{:.8}", wei_to_eth(gas_estimate.network_fee)),
                    total_fee_wei: total_fee.to_string(),
                    total_fee_eth: format!("{:.8}", wei_to_eth(total_fee)),
                },

                gas: GasPricing {
                    estimated_gas: gas_estimate.gas_limit.to_string(),
                    gas_price: gas_estimate.gas_price.to_string(),
                    max_fee_per_gas: gas_estimate.max_fee_per_gas.to_string(),
                    max_priority_fee_per_gas: gas_estimate.max_priority_fee_per_gas.to_string(),
                },

                exchange_rate,
                exchange_rate_expires_at,
                recipient_address: request.recipient_address,
                status,
                message: None,
            })
        })
    }).await
}

fn wei_to_eth(wei: u128) -> f64 {
    wei as f64 / 1e18
}

fn early_exit_if_wallets_invalid(request: &TransactionEstimateRequest) -> Option<TransactionEstimateResponse> {
    if !validate_wallet_address(&request.sender_address)
        || !validate_wallet_address(&request.recipient_address)
    {
        let mut response = TransactionEstimateResponse::default();
        response.token_type = request.token_type.clone();
        response.fiat_amount_minor = request.fiat_value;
        response.fiat_currency = request.fiat_currency.clone();
        response.status = EstimateFlags::SUCCESS | EstimateFlags::WALLET_NOT_FOUND;
        Some(response)
    } else {
        None
    }
}

fn validate_wallet_address(address: &str) -> bool {
    // Try parsing the address as a checksummed EIP-55 address
    Address::from_str(address).is_ok()
}
pub fn infer_estimate_success(current_flags: EstimateFlags) -> EstimateFlags {
    let mut final_flags = current_flags;

    // These flags indicate that some critical part of the estimate failed
    let fatal_errors = [
        EstimateFlags::INTERNAL_ERROR,
        EstimateFlags::CONTRACT_REVERTED,
        EstimateFlags::EXECUTION_REVERTED,
        EstimateFlags::RATE_LIMITED,
        EstimateFlags::RPC_AUTHENTICATION_FAILED,
        EstimateFlags::NONCE_ERROR,
        EstimateFlags::WALLET_NOT_FOUND,
    ];

    // If any of those are set, we do not set SUCCESS
    let has_fatal_error = fatal_errors.iter().any(|f| final_flags.contains(*f));

    if !has_fatal_error {
        final_flags.insert(EstimateFlags::SUCCESS);
    }

    final_flags
}


#[cfg(test)]
mod tests {
    use foxy_shared::services::authentication::generate_tokens;
use dotenv::dotenv;
use super::*;
    use foxy_shared::models::transactions::TokenType;
    use foxy_shared::services::cloudwatch_services::create_cloudwatch_client;
    use foxy_shared::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};

    #[tokio::test]
    async fn integration_test() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        let valid_request = TransactionEstimateRequest {
            fiat_value: 100_00,
            fiat_currency: "GBP".to_string(),
            sender_address: "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC".to_string(),
            recipient_address: "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7".to_string(),
            token_type: TokenType::ETH,
            transaction_value: None,
        };

        match estimate_transaction(&access_token, valid_request.clone(), &dynamodb_client, &create_cloudwatch_client().await).await {
            Ok(response) => {

                // Identity and base currency checks
                assert_eq!(response.token_type, TokenType::ETH);
                assert_eq!(response.fiat_amount_minor, 10_000);
                assert_eq!(response.fiat_currency, "GBP");
                assert_eq!(response.recipient_address, valid_request.recipient_address);

                // Amount estimates
                assert!(!response.eth_amount.is_empty(), "eth_amount should not be empty");
                assert!(!response.wei_amount.is_empty(), "wei_amount should not be empty");
                assert!(response.exchange_rate > 0.0, "exchange_rate should not be empty");

                // Fee breakdown checks
                assert!(!response.fees.service_fee_wei.is_empty(), "service_fee_wei should not be empty");
                assert!(!response.fees.network_fee_wei.is_empty(), "network_fee_wei should not be empty");
                assert!(!response.fees.total_fee_wei.is_empty(), "total_fee_wei should not be empty");
                assert!(!response.fees.service_fee_eth.is_empty(), "service_fee_eth should not be empty");
                assert!(!response.fees.network_fee_eth.is_empty(), "network_fee_eth should not be empty");
                assert!(!response.fees.total_fee_eth.is_empty(), "total_fee_eth should not be empty");

                // Gas pricing checks
                assert!(!response.gas.estimated_gas.is_empty(), "estimated_gas should not be empty");
                assert!(!response.gas.gas_price.is_empty(), "gas_price should not be empty");
                assert!(!response.gas.max_fee_per_gas.is_empty(), "max_fee_per_gas should not be empty");
                assert!(!response.gas.max_priority_fee_per_gas.is_empty(), "max_priority_fee_per_gas should not be empty");

                // Status flags
                assert!(
                    response.status.contains(EstimateFlags::SUCCESS),
                    "EstimateFlags should include SUCCESS"
                );

                // Exchange rate expiry check
                let now = chrono::Utc::now();
                assert!(
                    response.exchange_rate_expires_at > now,
                    "exchange_rate_expires_at should be in the future"
                );
            }

            Err(e) => {
                panic!("Expected successful estimate, got error: {:?}", e);
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_estimate_very_small_amount() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
        let access_token = token_result.access_token.expect("Access token missing");

        let request = TransactionEstimateRequest {
            fiat_value: 1,  // 1 penny
            fiat_currency: "GBP".to_string(),
            sender_address: "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC".to_string(),
            recipient_address: "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7".to_string(),
            token_type: TokenType::ETH,
            transaction_value: None,
        };

        let response = estimate_transaction(&access_token, request.clone(), &dynamodb_client, &create_cloudwatch_client().await).await
            .expect("Expected successful estimate");

        assert_eq!(response.fiat_amount_minor, 1);
        assert!(
            response.wei_amount.parse::<u128>().unwrap_or(0) > 0,
            "wei_amount should be > 0 even for small fiat values"
        );
        assert!(
            response.fees.service_fee_wei.parse::<u128>().unwrap_or(0) > 0,
            "service_fee_wei should still apply"
        );
        assert!(
            response.status.contains(EstimateFlags::SUCCESS),
            "Should include SUCCESS status"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_estimate_very_large_amount() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let token_result = generate_tokens(&cognito_client, &test_user_id).await?;
        let access_token = token_result.access_token.expect("Access token missing");

        let request = TransactionEstimateRequest {
            fiat_value: 100_000_00,  // £1,000,000 in pence
            fiat_currency: "GBP".to_string(),
            sender_address: "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC".to_string(),
            recipient_address: "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7".to_string(),
            token_type: TokenType::ETH,
            transaction_value: None,
        };

        let response = estimate_transaction(&access_token, request.clone(), &dynamodb_client, &create_cloudwatch_client().await).await
            .expect("Expected successful estimate");

        let wei = response.wei_amount.parse::<u128>().unwrap_or(0);
        let service_fee = response.fees.service_fee_wei.parse::<u128>().unwrap_or(0);

        assert!(wei > 10u128.pow(18), "Should be worth more than 1 ETH");
        assert!(service_fee > 0, "Large value should have a substantial service fee");
        assert!(
            response.status.contains(EstimateFlags::SUCCESS),
            "Should include SUCCESS status"
        );

        Ok(())
    }

    
    #[test]
    fn test_fiat_to_wei_conversion() {
        struct TestCase {
            fiat_amount: u64,   // Minor units (£10.00 → 1000)
            exchange_rate: f64, // Exchange rate (£2000 per ETH)
            expected_wei: u128, // Expected WEI output
        }

        let test_cases = vec![
            TestCase {
                fiat_amount: 1000, // £10.00 in minor units
                exchange_rate: 2000.0, // 1 ETH = £2000
                expected_wei: (1000u128 * 10u128.pow(18)) / (2000.0 * 100.0) as u128, // 0.005 ETH in WEI
            },
            TestCase {
                fiat_amount: 500, // £5.00
                exchange_rate: 2500.0, // 1 ETH = £2500
                expected_wei: (500u128 * 10u128.pow(18)) / (2500.0 * 100.0) as u128, // 0.002 ETH in WEI
            },
            TestCase {
                fiat_amount: 10000, // £100.00
                exchange_rate: 4000.0, // 1 ETH = £4000
                expected_wei: (10000u128 * 10u128.pow(18)) / (4000.0 * 100.0) as u128, // 0.025 ETH in WEI
            },
        ];

        for case in test_cases {
            let wei = (case.fiat_amount as u128) * 10u128.pow(18) / ((case.exchange_rate * 100.0) as u128);
            assert_eq!(
                wei, case.expected_wei,
                "Failed for fiat_amount: {}, exchange_rate: {}",
                case.fiat_amount, case.exchange_rate
            );
        }
    }
}