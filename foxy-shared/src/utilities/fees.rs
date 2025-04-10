use std::collections::HashMap;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_dynamodb::types::AttributeValue;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::database::errors::DynamoDbError;
use crate::services::cloudwatch_services::{result_to_f64, OperationMetricTracker};
use crate::utilities::config::get_env_var;

#[derive(Debug, Deserialize, Serialize)]
pub struct FeeStructure {
    pub base_fee_wei: u128,        // Stored in wei
    pub percentage_fee_bps: u64,   // Stored in basis points (e.g., 100 for 1%)
}

#[async_trait::async_trait]
pub trait FeeFetcher: Send + Sync {
    async fn fetch_fees(&self) -> Result<FeeStructure, DynamoDbError>;
}

#[async_trait::async_trait]
impl FeeFetcher for DynamoDbClient {
    async fn fetch_fees(&self) -> Result<FeeStructure, DynamoDbError> {
        let table_name = get_env_var("FEE_STRUCTURE_TABLE_NAME");
        let now = Utc::now().to_rfc3339(); // Get current UTC timestamp in ISO8601

        let result = self
            .query()
            .table_name(&table_name)
            .key_condition_expression("#fee = :fee_type AND #valid_from <= :now")
            .set_expression_attribute_names(Some(
                HashMap::from([
                    ("#fee".to_string(), "fee_type".to_string()),
                    ("#valid_from".to_string(), "valid_from".to_string())
                ])
            ))
            .expression_attribute_values(":fee_type", AttributeValue::S("service_fee".to_string()))
            .expression_attribute_values(":now", AttributeValue::S(now))
            .scan_index_forward(false)
            .limit(1)
            .send()
            .await
            .map_err(|e| {
                log::error!("🔥 Detailed DynamoDB query failure: {:#?}", e);
                DynamoDbError::from(e)
            })?;

        let latest_fee = result.items.and_then(|mut items| items.pop()).ok_or(DynamoDbError::NotFound)?;

        let base_fee_wei = latest_fee.get("base_fee")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u128>().ok())
            .unwrap_or(50);

        let percentage_fee_bps = latest_fee.get("percentage_fee")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1);

        Ok(FeeStructure {
            base_fee_wei,
            percentage_fee_bps,
        })
    }
}

pub async fn get_latest_fee_structure(
    dynamo_client: &dyn FeeFetcher,
    fetch_fee: impl Fn() -> Option<FeeStructure>) -> Result<FeeStructure, DynamoDbError> {
    if let Some(mocked_fee) = fetch_fee() {
        return Ok(mocked_fee);
    }

    dynamo_client.fetch_fees().await.map_err(DynamoDbError::from)
}

pub async fn calculate_service_fee(
    dynamo_client: &dyn FeeFetcher,
    wei_amount: u128,
) -> Result<u128, DynamoDbError> {
    let tracker = OperationMetricTracker::build("Fee").await;

    let result = get_latest_fee_structure(dynamo_client, || None).await
        .map(|fees| {
            let percentage_fee = (wei_amount * fees.percentage_fee_bps as u128) / 10_000;
            fees.base_fee_wei + percentage_fee
        });

    tracker.track(&result, result_to_f64(&result)).await;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    use dotenv::dotenv;
    use crate::utilities::test::get_dynamodb_client_with_assumed_role;

    pub struct MockDynamoDbClient;

    #[async_trait::async_trait]
    impl FeeFetcher for MockDynamoDbClient {
        async fn fetch_fees(&self) -> Result<FeeStructure, DynamoDbError> {
            Ok(FeeStructure {
                base_fee_wei: 0,
                percentage_fee_bps: 25,
            })
        }
    }

    /// ✅ Mock function returning a predefined fee structure
    fn mock_fee_structure() -> Option<FeeStructure> {
        Some(FeeStructure {
            base_fee_wei: 0,
            percentage_fee_bps: 25,
        })
    }

    #[tokio::test]
    async fn test_get_latest_fee_structure_with_mocked_fee() {
        let dynamo_client = MockDynamoDbClient;

        let result = get_latest_fee_structure(&dynamo_client, mock_fee_structure).await;
        assert!(result.is_ok());

        let fee = result.unwrap();
        assert_eq!(fee.base_fee_wei, 0, "Base fee should match mock data");
        assert_eq!(fee.percentage_fee_bps, 25, "Percentage fee should match mock data");
    }

    #[tokio::test]
    async fn test_get_latest_fee_structure_without_mocked_fee() {
        let dynamo_client = MockDynamoDbClient;

        let result = get_latest_fee_structure(&dynamo_client, || None).await;
        assert!(result.is_err(), "Should return an error if no mock fee is provided");
    }

    #[tokio::test]
    async fn test_service_fee_calculation() {
        dotenv().ok();
        let dynamo_client = MockDynamoDbClient;
        let fiat_amount = 10_000;
        let expected_fee = 10_025;

        let service_fee = calculate_service_fee(&dynamo_client, fiat_amount).await;
        assert_eq!(service_fee.unwrap(), expected_fee, "Incorrect service fee calculation");
    }

    #[tokio::test]
    async fn integration_test() {
        dotenv().ok();
        let _ = tracing_subscriber::fmt::try_init();
        log::error!("Starting integration test");
        let dynamodb_client = get_dynamodb_client_with_assumed_role().await;
        let fiat_amount = 10_000;

        let service_fee = calculate_service_fee(&dynamodb_client, fiat_amount).await;

        match service_fee{
            Ok(_) => {
                /* do nothing, we will assert shortly */
            },
            Err(ref e) => {log::error!("{}", e)},
        }

        assert!(service_fee.is_ok(), "Incorrect service fee calculation");
    }
}