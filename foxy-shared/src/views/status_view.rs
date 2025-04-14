use std::collections::HashMap;
use std::sync::Arc;

use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use aws_sdk_dynamodb::operation::query::QueryOutput;
use aws_sdk_dynamodb::types::Select;
use base64::Engine;
use crate::models::transactions::{Transaction, TransactionStatus};
use crate::state_machine::transaction_event_factory::TransactionEvent;
use crate::database::transaction_event::TransactionEventManager;
use tracing::{debug, info};

pub struct TransactionStatusViewManager {
    table_name: String,
    dynamo_db_client: Arc<DynamoDbClient>,
    tem: Arc<TransactionEventManager>,
}

pub struct WalletQueryResult {
    pub transaction_ids: Vec<String>,
    pub next_page_token: Option<String>,
}

impl TransactionStatusViewManager {
    pub fn new(table_name: String, dynamo_db_client: Arc<DynamoDbClient>, tem: Arc<TransactionEventManager>) -> Self {
        Self { table_name, dynamo_db_client, tem }
    }

    pub async fn project(&self, transaction_id: &str) -> Result<(), anyhow::Error> {
        let latest_event = self.tem.get_latest_event(transaction_id).await?;
        let tx = &latest_event.transaction;

        let item = self.to_dynamo_item(&latest_event, tx)?;

        self.dynamo_db_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        info!(tx_id = %transaction_id, status = ?latest_event.status, "📌 Projected status view");
        Ok(())
    }

    fn to_dynamo_item(
        &self,
        event: &TransactionEvent,
        tx: &Transaction,
    ) -> Result<std::collections::HashMap<String, AttributeValue>, anyhow::Error> {
        let mut item = std::collections::HashMap::new();
        item.insert("PK".to_string(), AttributeValue::S(format!("Transaction#{}", tx.transaction_id)));
        item.insert("Status".to_string(), AttributeValue::S(event.status.to_string()));
        item.insert("UpdatedAt".to_string(), AttributeValue::S(event.created_at.to_rfc3339()));
        item.insert("UserID".to_string(), AttributeValue::S(event.user_id.clone()));

        if let Some(ref hash) = tx.tx_hash() {
            item.insert("TxHash".to_string(), AttributeValue::S(hash.clone().to_string()));
        }
        if let Some(block) = tx.block_number {
            item.insert("BlockNumber".to_string(), AttributeValue::N(block.to_string()));
        }

        Ok(item)
    }

    pub async fn query_by_status(
        &self,
        status: TransactionStatus,
    ) -> Result<Vec<String>, anyhow::Error> {
        let status_str = status.to_string();

        let result = self
            .dynamo_db_client
            .query()
            .table_name(&self.table_name)
            .index_name("StatusIndex")
            .key_condition_expression("Status = :status")
            .expression_attribute_values(":status", AttributeValue::S(status_str))
            .select(Select::AllAttributes)
            .send()
            .await?;

        let tx_ids: Vec<String> = result
            .items()
            .iter()
            .filter_map(|item| item.get("PK"))
            .filter_map(|pk| pk.as_s().ok())
            .filter_map(|s| s.strip_prefix("Transaction#"))
            .map(|s| s.to_string())
            .collect();

        debug!(count = tx_ids.len(), "Found {} transactions with status {:?}", tx_ids.len(), status);

        Ok(tx_ids)
    }

    pub async fn query_by_wallet(&self, wallet_address: &str, limit: Option<i32>) -> Result<WalletQueryResult, anyhow::Error> {
        self.query_by_wallet_and_status(wallet_address, None, None, limit).await
    }

    pub async fn query_by_wallet_and_status(
        &self,
        wallet_address: &str,
        status: Option<TransactionStatus>,
        last_evaluated_key: Option<HashMap<String, AttributeValue>>,
        limit: Option<i32>,
    ) -> Result<WalletQueryResult, anyhow::Error> {
        let sender_fut = self.query_gsi("SenderIndex", "SenderAddress", wallet_address, status.clone(), last_evaluated_key.clone(), limit);
        let recipient_fut = self.query_gsi("RecipientIndex", "RecipientAddress", wallet_address, status.clone(), last_evaluated_key, limit);

        let (sender_result, recipient_result) = tokio::join!(sender_fut, recipient_fut);

        let mut tx_ids: Vec<String> = vec![];
        tx_ids.extend(sender_result?.transaction_ids);
        tx_ids.extend(recipient_result?.transaction_ids);

        tx_ids.sort();
        tx_ids.dedup();

        // Note: Pagination key cannot be combined safely, so we don't propagate it for combined queries
        Ok(WalletQueryResult {
            transaction_ids: tx_ids,
            next_page_token: None,
        })
    }

    pub fn decode_page_token(token: &str) -> Result<HashMap<String, AttributeValue>, anyhow::Error> {
        let decoded_bytes = base64::engine::general_purpose::STANDARD.decode(token)?;
        let intermediate: HashMap<String, String> = serde_json::from_slice(&decoded_bytes)?;
        let map = intermediate
            .into_iter()
            .map(|(k, v)| (k, AttributeValue::S(v)))
            .collect();
        Ok(map)
    }

    pub fn encode_page_token(key: &HashMap<String, AttributeValue>) -> Result<String, anyhow::Error> {
        let string_map: HashMap<String, String> = key
            .iter()
            .filter_map(|(k, v)| v.as_s().ok().map(|s| (k.clone(), s.to_string())))
            .collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&string_map)?);
        Ok(encoded)
    }

    async fn query_gsi(
        &self,
        index_name: &str,
        key: &str,
        value: &str,
        status: Option<TransactionStatus>,
        exclusive_start_key: Option<HashMap<String, AttributeValue>>,
        limit: Option<i32>,
    ) -> Result<WalletQueryResult, anyhow::Error> {
        let mut builder = self.dynamo_db_client.query()
            .table_name(&self.table_name)
            .index_name(index_name)
            .key_condition_expression(format!("{} = :v", key))
            .expression_attribute_values(":v", AttributeValue::S(value.to_string()))
            .select(Select::AllAttributes);

        if let Some(s) = &status {
            builder = builder
                .filter_expression("Status = :s")
                .expression_attribute_values(":s", AttributeValue::S(s.to_string()));
        }

        if let Some(start_key) = exclusive_start_key {
            builder = builder.set_exclusive_start_key(Some(start_key));
        }

        if let Some(l) = limit {
            builder = builder.limit(l);
        }

        let result: QueryOutput = builder.send().await?;

        let tx_ids = result
            .items()
            .iter()
            .filter_map(|item| item.get("PK"))
            .filter_map(|pk| pk.as_s().ok())
            .filter_map(|s| s.strip_prefix("Transaction#"))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let next_page_token = result.last_evaluated_key().map(|key| {
            Self::encode_page_token(key).unwrap_or_else(|_| "".to_string())
        });

        debug!(count = tx_ids.len(), index = index_name, ?status, limit, ?next_page_token, "Found transactions via {} with status {:?} and limit {:?}", index_name, status, limit);

        Ok(WalletQueryResult {
            transaction_ids: tx_ids,
            next_page_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::utilities::config;
    use crate::utilities::config::get_transaction_event_table;
    use crate::utilities::test::{get_dynamodb_client_with_assumed_role, init_tracing};
    use super::*;

    #[test]
    fn test_encode_decode_token_roundtrip() {
        let mut original: HashMap<String, AttributeValue> = HashMap::new();
        original.insert("PK".to_string(), AttributeValue::S("Transaction#abc123".to_string()));
        original.insert("SK".to_string(), AttributeValue::S("Event#2025-01-01T00:00:00Z".to_string()));

        let encoded = TransactionStatusViewManager::encode_page_token(&original).unwrap();
        let decoded = TransactionStatusViewManager::decode_page_token(&encoded).unwrap();

        assert_eq!(decoded.get("PK").unwrap().as_s().unwrap(), "Transaction#abc123");
        assert_eq!(decoded.get("SK").unwrap().as_s().unwrap(), "Event#2025-01-01T00:00:00Z");
    }

    #[tokio::test]
    #[ignore]
    async fn test_query_by_wallet_live() {
        config::init();
        init_tracing();

        let dynamo_db_client = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let view = TransactionStatusViewManager {
            table_name: std::env::var("STATUS_VIEW_TABLE").unwrap_or_else(|_| "foxy_dev_TransactionStatusView".to_string()),
            dynamo_db_client: dynamo_db_client.clone(),
            tem: TransactionEventManager::new(dynamo_db_client.clone(), get_transaction_event_table()),
        };

        let wallet = "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0";

        match view.query_by_wallet(wallet, Some(5)).await {
            Ok(result) => {
                info!(count = result.transaction_ids.len(), ?result.next_page_token, "Integration test passed");
                assert!(true);
            }
            Err(e) => {
                tracing::error!(?e, "❌ Integration test failed");
                panic!("query_by_wallet failed: {:?}", e);
            }
        }
    }
}
