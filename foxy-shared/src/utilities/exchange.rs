use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use once_cell::sync::Lazy;
use crate::models::errors::FetchRateError;
use crate::models::transactions::TokenType;
use crate::services::cloudwatch_services::OperationMetricTracker;

static SHARED_CLIENT: Lazy<Client> = Lazy::new(Client::new);

#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeRateResponse {
    pub rate: f64,
}

#[derive(Debug, Deserialize)]
struct CoinbaseResponse {
    data: CoinbaseData,
}

#[derive(Debug, Deserialize)]
struct CoinbaseData {
    rates: HashMap<String, String>,
}

//TODO: Should these be environment variables?
const FALLBACK_API: &str = "https://api.coinbase.com/v2/exchange-rates?currency=ETH";


pub struct ExchangeRateManager {
    client: Client,
}

impl ExchangeRateManager {
    pub fn new() -> Self {
        Self {
            client: SHARED_CLIENT.clone()
        }
    }

    //TODO: Get exchange rates for other tokens
    pub async fn get_latest_rate(&self, fiat_currency: &str, _token_type: &TokenType) -> Result<f64, FetchRateError> {
        let tracker = OperationMetricTracker::build("ExchangeRate").await;

        let result = self.fetch_exchange_rate(
            || async { self.fetch_chainlink_rate(fiat_currency).await },
            || async { self.fetch_coinbase_rate(fiat_currency).await },
        ).await;

        // Emit fatal if both failed
        if result.is_err() {
            tracker.emit_fatal("ExchangeRate").await;
        }

        let rate_opt = result.as_ref().ok().copied();
        tracker.track(&result, rate_opt).await;

        result
    }

    async fn fetch_exchange_rate<F, Fut, G, Fut2>(&self,
        fetch_chainlink: F,
        fetch_coinbase: G,
    ) -> Result<f64, FetchRateError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output=Result<f64, FetchRateError>>,
        G: Fn() -> Fut2,
        Fut2: Future<Output=Result<f64, FetchRateError>>,
    {
        if let Ok(rate) = fetch_chainlink().await {
            return Ok(rate);
        }

        if let Ok(rate) = fetch_coinbase().await {
            return Ok(rate);
        }

        Err(FetchRateError::MissingRate)
    }

    fn chainlink_url(fiat_currency: &str) -> String {
        format!("https://api.chainlink.com/eth-{}", fiat_currency.to_lowercase())
    }

    async fn fetch_chainlink_rate(&self, fiat_currency: &str) -> Result<f64, FetchRateError> {
        let url = Self::chainlink_url(fiat_currency);
        let client = &self.client;
        let response: ExchangeRateResponse = client.get(url).send().await?.json().await?;
        Ok(response.rate)
    }

    async fn fetch_coinbase_rate(&self, fiat_currency: &str) -> Result<f64, FetchRateError> {
        let client = &self.client;
        let url = format!("{}", FALLBACK_API);

        let response: CoinbaseResponse = client.get(&url).send().await?.json().await?;

        response.data.rates
            .get(&fiat_currency.to_uppercase())
            .and_then(|rate| rate.parse::<f64>().ok())
            .ok_or(FetchRateError::MissingRate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::errors::FetchRateError;
    use tokio;

    fn mock_success_chainlink() -> impl Future<Output = Result<f64, FetchRateError>> {
        async { Ok(2700.0) }
    }

    fn mock_success_coinbase() -> impl Future<Output = Result<f64, FetchRateError>> {
        async { Ok(2650.0) }
    }

    fn mock_fail() -> impl Future<Output = Result<f64, FetchRateError>> {
        async { Err(FetchRateError::MissingRate) }
    }

    #[tokio::test]
    async fn test_fetch_chainlink_success() {
        let erm = ExchangeRateManager::new();
        let result = erm.fetch_exchange_rate(|| mock_success_chainlink(), || mock_success_coinbase()).await;
        assert_eq!(result.unwrap(), 2700.0, "Should return Chainlink rate first");
    }

    #[tokio::test]
    async fn test_fetch_coinbase_fallback() {
        let erm = ExchangeRateManager::new();
        let result = erm.fetch_exchange_rate(|| mock_fail(), || mock_success_coinbase()).await;
        assert_eq!(result.unwrap(), 2650.0, "Should fallback to Coinbase if Chainlink fails");
    }

    #[tokio::test]
    async fn test_fetch_exchange_rate_failure() {
        let erm = ExchangeRateManager::new();
        let result = erm.fetch_exchange_rate(|| mock_fail(), || mock_fail()).await;
        assert!(matches!(result, Err(FetchRateError::MissingRate)), "Should error if both sources fail");
    }
}