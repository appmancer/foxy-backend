use std::collections::HashMap;
use std::sync::Arc;

use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use aws_sdk_dynamodb::types::Select;
use base64::Engine;
use crate::models::transactions::{TransactionEvent, TransactionHistoryItem};
use tracing::{info, warn};

pub struct TransactionHistoryViewManager {
    table_name: String,
    dynamo_db_client: Arc<DynamoDbClient>,
}

pub struct Paginated<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}

impl TransactionHistoryViewManager {
    pub fn new(table_name: String, dynamo_db_client: Arc<DynamoDbClient>) -> Self {
        Self { table_name, dynamo_db_client }
    }

    pub async fn get_by_bundle_id_for_user(
        &self,
        user_id: &str,
        bundle_id: &str,
    ) -> Result<Option<TransactionHistoryItem>, anyhow::Error> {
        let pk = format!("User#{}", user_id);
        let prefix = format!("Bundle#{}", bundle_id);

        let result = self.dynamo_db_client.query()
            .table_name(&self.table_name)
            .key_condition_expression("PK = :pk AND begins_with(SK, :prefix)")
            .expression_attribute_values(
                ":pk", AttributeValue::S(pk.clone()),
            )
            .expression_attribute_values(
                ":prefix", AttributeValue::S(prefix.clone()),
            )
            .limit(1)
            .send()
            .await?;

        if let Some(item) = result.items().first() {
            Ok(Self::parse_history_item(item))
        } else {
            Ok(None)
        }
    }

    pub async fn project_from_event(&self, event: &TransactionEvent) -> Result<(), anyhow::Error> {
        let metadata = event.bundle_snapshot.metadata.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing bundle metadata"))?;

        let sender = metadata.sender.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing sender in metadata"))?;

        let recipient = metadata.recipient.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing recipient in metadata"))?;

        let senderid = &sender.user_id;
        let recipientid = &recipient.user_id;

        let sender_item = TransactionHistoryItem::from_event_and_user(event, senderid);
        let recipient_item = TransactionHistoryItem::from_event_and_user(event, recipientid);

        let mut tasks = vec![];

        if let Some(sender_view) = sender_item {
            let pk = format!("User#{}", sender_view.counterparty.user_id);
            let sk = format!("Bundle#{}|{}", sender_view.bundle_id, sender_view.timestamp);
            let item = Self::to_dynamo_item(&pk, &sk, &sender_view)?;
            info!(?item, "Writing item to History View");
            tasks.push(self.write_item(item));
        }

        if let Some(recipient_view) = recipient_item {
            let pk = format!("User#{}", recipient_view.counterparty.user_id);
            let sk = format!("Bundle#{}|{}", recipient_view.bundle_id, recipient_view.timestamp);
            let item = Self::to_dynamo_item(&pk, &sk, &recipient_view)?;
            info!(?item, "Writing item to History View");
            tasks.push(self.write_item(item));
        }

        futures::future::try_join_all(tasks).await?;

        info!(bundle_id = %event.bundle_id, "✅ Projected history view for both parties");
        Ok(())
    }

    pub fn encode_page_token(key: &HashMap<String, AttributeValue>) -> Result<String, anyhow::Error> {
        let string_map: HashMap<String, String> = key
            .iter()
            .filter_map(|(k, v)| v.as_s().ok().map(|s| (k.clone(), s.to_string())))
            .collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&string_map)?);
        Ok(encoded)
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

    fn parse_history_item(item: &HashMap<String, AttributeValue>) -> Option<TransactionHistoryItem> {
        Some(TransactionHistoryItem {
            bundle_id: item.get("BundleID")?.as_s().ok()?.clone(),
            direction: item.get("Direction")?.as_s().ok()?.parse().ok()?,
            status: item.get("Status")?.as_s().ok()?.parse().ok()?,
            amount: item.get("Amount")?.as_n().ok()?.parse().ok()?,
            token: item.get("Token")?.as_s().ok()?.clone(),
            timestamp: item.get("Timestamp")?.as_s().ok()?.clone(),
            counterparty: crate::models::transactions::PartyDetails {
                user_id: item.get("CounterpartyID")?.as_s().ok()?.clone(),
                name: item.get("CounterpartyName")?.as_s().ok()?.clone(),
                wallet: item.get("CounterpartyWallet")?.as_s().ok()?.clone(),
            },
            message: item.get("Message").and_then(|v| v.as_s().ok()).map(String::from),
            tx_hash: item.get("TxHash").and_then(|v| v.as_s().ok()).map(String::from),
            display_total_fee: item.get("DisplayTotalFee")?.as_s().ok()?.clone(),
            service_fee_minor: item.get("ServiceFeeMinor")?.as_n().ok()?.parse().ok()?,
            total_fiat_minor: item.get("TotalFiatMinor")?.as_n().ok()?.parse().ok()?,
            fee_tx_value_eth: item.get("FeeTxValueEth")?.as_s().ok()?.clone(),
        })
    }

    pub async fn query_by_user(
        &self,
        user_id: &str,
        limit: Option<i32>,
        last_evaluated_key: Option<HashMap<String, AttributeValue>>,
    ) -> Result<Paginated<TransactionHistoryItem>, anyhow::Error> {
        let pk = format!("User#{}", user_id);
        let mut builder = self.dynamo_db_client.query()
            .table_name(&self.table_name)
            .key_condition_expression("PK = :pk")
            .expression_attribute_values(":pk", AttributeValue::S(pk))
            .select(Select::AllAttributes);

        if let Some(start_key) = last_evaluated_key {
            builder = builder.set_exclusive_start_key(Some(start_key));
        }

        if let Some(l) = limit {
            builder = builder.limit(l);
        }

        let result = builder.send().await?;

        let mut items = Vec::new();
        for item in result.items().iter() {
            if let Some(t) = Self::parse_history_item(item) {
                items.push(t);
            } else {
                warn!(?item, "❌ Failed to parse TransactionHistoryItem from DynamoDB row");
            }
        }

        let next_page_token = result.last_evaluated_key().map(|key| {
            Self::encode_page_token(key).unwrap_or_else(|_| "".to_string())
        });

        Ok(Paginated { items, next_page_token })
    }

    async fn write_item(&self, item: HashMap<String, AttributeValue>) -> Result<(), anyhow::Error> {
        info!(?item, "Writing item to History View");
        info!("{}", self.table_name.as_str());

        self.dynamo_db_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;
        Ok(())
    }

    fn to_dynamo_item(
        pk: &str,
        sk: &str,
        view: &TransactionHistoryItem,
    ) -> Result<HashMap<String, AttributeValue>, anyhow::Error> {
        let mut item = HashMap::new();
        item.insert("PK".to_string(), AttributeValue::S(pk.to_string()));
        item.insert("SK".to_string(), AttributeValue::S(sk.to_string()));
        item.insert("BundleID".to_string(), AttributeValue::S(view.bundle_id.clone()));
        item.insert("Direction".to_string(), AttributeValue::S(view.direction.to_string()));
        item.insert("Status".to_string(), AttributeValue::S(view.status.to_string()));
        item.insert("Amount".to_string(), AttributeValue::N(view.amount.to_string()));
        item.insert("Token".to_string(), AttributeValue::S(view.token.clone()));
        item.insert("Timestamp".to_string(), AttributeValue::S(view.timestamp.clone()));
        item.insert("CounterpartyID".to_string(), AttributeValue::S(view.counterparty.user_id.clone()));
        item.insert("CounterpartyName".to_string(), AttributeValue::S(view.counterparty.name.clone()));
        item.insert("CounterpartyWallet".to_string(), AttributeValue::S(view.counterparty.wallet.clone()));
        item.insert("DisplayTotalFee".to_string(), AttributeValue::S(view.display_total_fee.clone()));
        item.insert("ServiceFeeMinor".to_string(), AttributeValue::N(view.service_fee_minor.to_string()));
        item.insert("TotalFiatMinor".to_string(), AttributeValue::N(view.total_fiat_minor.to_string()));
        item.insert("FeeTxValueEth".to_string(), AttributeValue::S(view.fee_tx_value_eth.clone()));

        if let Some(ref message) = view.message {
            item.insert("Message".to_string(), AttributeValue::S(message.clone()));
        }
        if let Some(ref tx_hash) = view.tx_hash {
            item.insert("TxHash".to_string(), AttributeValue::S(tx_hash.clone()));
        }

        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use super::*;
    use crate::models::transactions::{BundleMetadata, BundleStatus, Direction, EventType, GasPricing, PartyDetails, Transaction, TransactionBundle, TransactionStatus};
    use crate::models::user_device::UserDevice;
    use crate::utilities::config;
    use crate::utilities::config::get_history_view_table;


    fn mock_event(sender_id: &str, recipient_id: &str) -> TransactionEvent {
        let sender = PartyDetails {
            user_id: sender_id.to_string(),
            name: "Sender Name".to_string(),
            wallet: "0xSender".to_string(),
        };

        let recipient = PartyDetails {
            user_id: recipient_id.to_string(),
            name: "Recipient Name".to_string(),
            wallet: "0xRecipient".to_string(),
        };

        let user_device = UserDevice::new("0eacf2aa-e788-4b54-bc1c-a95a05fc7d62".to_string(),
                                         "f30M3RyRSpKlDY7lbJBBKu:APA91bGH7m_zXvyYsCHdE5L7DDaT4ObWIe9y_5d3JKANJiM0zC6BJYcrTn1h9cfcaFgpK_hg2Sc32V951WQbP_kuv6ZwjITkhORb7G2pzx1RvbSsVyiu5eI".to_string(),
                                         "Android".to_string(), "0.1.0".to_string());

        let metadata = BundleMetadata {
            display_currency: "GBP".to_string(),
            expected_currency_amount: 1000,
            message: Some("Test".to_string()),
            sender: Some(sender.clone()),
            recipient: Some(recipient.clone()),
            app_version: None,
            location: None,
            service_fee: 100000000000000u128,
            exchange_rate: 1370.0,
            gas_pricing: GasPricing {
                estimated_gas: "21000".to_string(),
                gas_price: "1000000".to_string(),
                max_fee_per_gas: "1100000".to_string(),
                max_priority_fee_per_gas: "150000".to_string(),
            },
            service_fee_minor: Some(20),
            user_device,
        };

        let bundle = TransactionBundle {
            bundle_id: "test-bundle-id".to_string(),
            user_id: sender_id.to_string(),
            status: BundleStatus::Initiated,
            fee_tx: Transaction::mock_fee(sender_id, 100000000000000u128),
            main_tx: Transaction::mock_main(sender_id, recipient_id, 5000000000000000u128),
            metadata: Some(metadata),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        TransactionEvent {
            event_id: "event-id".to_string(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: sender_id.to_string(),
            event_type: EventType::Initiate,
            leg: None,
            bundle_status: Some(BundleStatus::Initiated),
            transaction_status: None,
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        }
    }

    #[test]
    fn test_sender_view_projection() {
        let sender_id = "user_sender";
        let recipient_id = "user_recipient";
        let event = mock_event(sender_id, recipient_id);

        let view = TransactionHistoryItem::from_event_and_user(&event, sender_id).unwrap();
        assert_eq!(view.direction, Direction::Outgoing);
        assert_eq!(view.counterparty.user_id, recipient_id);
    }

    #[test]
    fn test_recipient_view_projection() {
        let sender_id = "user_sender";
        let recipient_id = "user_recipient";
        let event = mock_event(sender_id, recipient_id);

        let view = TransactionHistoryItem::from_event_and_user(&event, recipient_id).unwrap();
        assert_eq!(view.direction, Direction::Incoming);
        assert_eq!(view.counterparty.user_id, sender_id);
    }

    #[test]
    fn test_parse_history_item_happy_path() {
        let mut item = HashMap::new();
        item.insert("BundleID".to_string(), AttributeValue::S("bundle-123".to_string()));
        item.insert("Direction".to_string(), AttributeValue::S("outgoing".to_string()));
        item.insert("Status".to_string(), AttributeValue::S("Confirmed".to_string()));
        item.insert("Amount".to_string(), AttributeValue::N("0.5".to_string()));
        item.insert("Token".to_string(), AttributeValue::S("ETH".to_string()));
        item.insert("Timestamp".to_string(), AttributeValue::S("2025-04-23T12:00:00Z".to_string()));
        item.insert("CounterpartyID".to_string(), AttributeValue::S("user-456".to_string()));
        item.insert("CounterpartyName".to_string(), AttributeValue::S("Andrew".to_string()));
        item.insert("CounterpartyWallet".to_string(), AttributeValue::S("0xabc".to_string()));
        item.insert("Message".to_string(), AttributeValue::S("Lunch".to_string()));
        item.insert("TxHash".to_string(), AttributeValue::S("0xhash".to_string()));

        let parsed = TransactionHistoryViewManager::parse_history_item(&item).unwrap();

        assert_eq!(parsed.bundle_id, "bundle-123");
        assert_eq!(parsed.direction, Direction::Outgoing);
        assert_eq!(parsed.status, TransactionStatus::Confirmed);
        assert_eq!(parsed.amount, 0.5);
        assert_eq!(parsed.token, "ETH");
        assert_eq!(parsed.timestamp, "2025-04-23T12:00:00Z");
        assert_eq!(parsed.counterparty.user_id, "user-456");
        assert_eq!(parsed.counterparty.name, "Andrew");
        assert_eq!(parsed.counterparty.wallet, "0xabc");
        assert_eq!(parsed.message.as_deref(), Some("Lunch"));
        assert_eq!(parsed.tx_hash.as_deref(), Some("0xhash"));
    }

    #[tokio::test]
    async fn test_get_by_bundle_id_for_user_query() {
        config::init();
        use crate::utilities::test::get_dynamodb_client_with_assumed_role;

        let client = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let manager = TransactionHistoryViewManager::new(
            get_history_view_table(),
            client,
        );

        let user_id = "test-user";
        let bundle_id = "bundle-123";

        let result = manager.get_by_bundle_id_for_user(user_id, bundle_id).await;

        match result {
            Ok(Some(item)) => {
                println!("✅ Got transaction history item: {:?}", item);
                assert_eq!(item.bundle_id, bundle_id);
            },
            Ok(None) => println!("ℹ️ No item found for bundle_id: {}", bundle_id),
            Err(e) => panic!("❌ Failed to query by bundle id: {:?}", e),
        }
    }
}
