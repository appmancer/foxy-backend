use std::time::Instant;
use http::Response;
use lambda_http::{Body, Request};
use foxy_shared::services::cognito_services::{get_cognito_client, get_user_data};
use foxy_shared::models::errors::WalletError;
use foxy_shared::models::wallet::{BalanceResponse, FiatBalance};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::requests::extract_bearer_token;
use foxy_shared::utilities::responses::{error_response, success_response};
use foxy_shared::utilities::wallet::{format_wei_to_eth_f64, format_wei_to_eth_string, get_wallet_balance};
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_cloudwatch::types::StandardUnit;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use foxy_shared::models::transactions::TokenType;
use foxy_shared::utilities::exchange::ExchangeRateManager;

pub async fn handler(event: Request) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let client = get_cognito_client().await;
    let cloudwatch_client = create_cloudwatch_client().await;

    match token {
        Some(token) => match fetch_balance(token, &client, &cloudwatch_client).await {
            Ok(response) => success_response(response),
            Err(err) => error_response(format!("{:?}", err)),
        },
        _ => error_response("Missing authorization token"),
    }
}

async fn fetch_balance(token: &str, cognito_client: &CognitoClient, cloudwatch_client: &CloudWatchClient) -> Result<BalanceResponse, WalletError> {
    with_valid_user(token, |user_id| async move {
        log::info!("Fetching balance for user: {}", user_id);
        let start_time = Instant::now();

        let user_profile = get_user_data(cognito_client, &user_id)
            .await
            .map_err(|e| WalletError::MissingWallet(format!("Failed to fetch user data: {:?}", e)))?;

        let wallet_address = user_profile.wallet_address.unwrap();
        let default_currency = user_profile.currency.unwrap_or_else(|| "GBP".to_string());

        match get_wallet_balance(&wallet_address).await {
            Ok(balance) => {
                let eth = format_wei_to_eth_f64(balance); // you'll need this as f64
                let token_type = TokenType::ETH;

                let erm = ExchangeRateManager::new();
                let rate = erm.get_latest_rate(&default_currency, &token_type)
                    .await
                    .map_err(|e| WalletError::Network(format!("Exchange rate error: {}", e)))?;

                let fiat_value = eth * rate;

                let duration = start_time.elapsed().as_secs_f64();
                emit_metric(cloudwatch_client, "GetBalance", duration, StandardUnit::Seconds).await;
                Ok(BalanceResponse {
                    wei: balance.to_string(),
                    token: "ETH".to_string(),
                    balance: format_wei_to_eth_string(balance, 6),
                    fiat: FiatBalance {
                        value: format!("{:.2}", fiat_value),
                        currency: default_currency,
                    },
                })
            },
            Err(e) => Err(e)
        }
    }).await
}

#[cfg(test)]
mod tests {
    use foxy_shared::services::authentication::generate_tokens;
    use dotenv::dotenv;
    use super::*;
    use foxy_shared::services::cloudwatch_services::create_cloudwatch_client;
    use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;

    #[tokio::test]
    async fn integration_test() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let test_user_id = "108298283161988749543";
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");
        let cloudwatch_client = create_cloudwatch_client().await;

        match fetch_balance(&access_token, &cognito_client, &cloudwatch_client).await {
            Ok(balance) => {
                println!("Balance: {:?}", balance);
                assert!(balance.balance.len() > 0, "Balance does not exist");
                assert!(balance.wei.len() > 0, "Wei does not exist");
                assert_eq!(balance.token, "ETH", "Incorrect token");
                assert!(balance.balance.parse::<f64>().unwrap() > 0.0, "Balance does not exist");
            },
            Err(e) => {
                log::error!("Failed to fetch balance: {}", e);
                assert!(false, "Failed to fetch balance");
            }
        }

        Ok(())
    }
}