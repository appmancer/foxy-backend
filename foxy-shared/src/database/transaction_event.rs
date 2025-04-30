use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use log::info;
use uuid::Uuid;
use crate::database::errors::DynamoDbError;
use crate::models::errors::TransactionError;
use crate::models::transactions::{BundleStatus, EventType, TransactionBundle, TransactionEvent, TransactionLeg, TransactionStatus, TransactionStatusView};
use crate::utilities::config::{get_history_view_table, get_transaction_view_table};
use crate::views::history_view::TransactionHistoryViewManager;
use crate::views::status_view::TransactionStatusViewManager;

pub struct TransactionEventManager {
    client: Arc<DynamoDbClient>,
    table_name: String,
}

impl TransactionEventManager {
    // table_name is the event log table, probably from get_transaction_event_table()
    pub fn new(client: Arc<DynamoDbClient>, table_name: String) -> Arc<Self> {
        Arc::new(Self { client, table_name })
    }

    pub fn client(&self) -> Arc<DynamoDbClient> {
        self.client.clone()
    }
    pub async fn persist(
        self: Arc<Self>,
        event: &TransactionEvent,
    ) -> Result<String, DynamoDbError> {
        if !event.event_id.is_empty() {
            return Err(DynamoDbError::AlreadyPersisted(format!(
                "Attempted to persist an event that already has event_id: {}",
                event.event_id
            )));
        }

        let item = self.to_dynamo_item(event)?;

        //TODO: We should create constants for item fields
        let event_id_str = item.get("EventID")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing or invalid EventID".into()))?
            .to_string();

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(DynamoDbError::from)?;

        let projector = TransactionStatusViewManager::new(
            get_transaction_view_table(),
            self.client.clone(),
            self.clone(),
        );

        if let Err(e) = projector.project(&event.bundle_id).await {
            tracing::error!(?e, "Failed to project status view");
        }

        let history_view = TransactionHistoryViewManager::new(
            get_history_view_table(),
            self.client.clone(),
        );

        if let Err(e) = history_view.project_from_event(event).await {
            tracing::error!(?e, "Failed to project history view");
        }

        Ok(event_id_str)
    }

    fn to_dynamo_item(
        &self,
        event: &TransactionEvent,
    ) -> Result<HashMap<String, AttributeValue>, DynamoDbError> {
        let mut item = HashMap::new();
        let event_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();

        let bundle_json = serde_json::to_string(&event.bundle_snapshot)
            .map_err(|e| DynamoDbError::Serialization(e.to_string()))?;

        item.insert("PK".to_string(), AttributeValue::S(format!("Bundle#{}", event.bundle_id)));
        item.insert("SK".to_string(), AttributeValue::S(format!("Event#{}", timestamp)));

        item.insert("EventID".to_string(), AttributeValue::S(event_id));
        item.insert("UserID".to_string(), AttributeValue::S(event.user_id.clone()));
        item.insert("EventType".to_string(), AttributeValue::S(event.event_type.to_string()));
        item.insert("CreatedAt".to_string(), AttributeValue::S(timestamp));
        item.insert("BundleSnapshot".to_string(), AttributeValue::S(bundle_json));

        if let Some(leg) = event.leg {
            item.insert("Leg".to_string(), AttributeValue::S(leg.to_string()));
        }

        if let Some(tx_status) = &event.transaction_status {
            item.insert("TransactionStatus".to_string(), AttributeValue::S(tx_status.to_string()));
        }

        if let Some(bundle_status) = &event.bundle_status {
            item.insert("BundleStatus".to_string(), AttributeValue::S(bundle_status.to_string()));
        }

        Ok(item)
    }

    pub async fn persist_initial_event(self: Arc<Self>, bundle: &TransactionBundle) -> Result<(), DynamoDbError> {
        match TransactionEvent::initiate(bundle.clone()) {
            Ok(event) => {
                self.persist(&event).await?;
                Ok(())
            }
            Err(e) => { Err(DynamoDbError::DynamoDbOperation(format!("Unable to persist event: {}", e))) }
        }
    }

    pub async fn get_latest_event(
        &self,
        bundle_id: &str,
    ) -> Result<TransactionEvent, DynamoDbError> {
        let pk_value = format!("Bundle#{}", bundle_id);
        tracing::info!(%bundle_id, %pk_value, "🔎 Querying DynamoDB for latest event");
        tracing::info!(table = %self.table_name, "📋 Using table");

        let result = self.client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression("PK = :pk and begins_with(SK, :sk)")
            .expression_attribute_values(":pk", AttributeValue::S(format!("Bundle#{}", bundle_id)))
            .expression_attribute_values(":sk", AttributeValue::S("Event#".to_string()))
            .scan_index_forward(false)
            .limit(1)
            .send()
            .await
            .map_err(DynamoDbError::from)?;

        let item_count = result.items.as_ref().map(|items| items.len()).unwrap_or(0);
        tracing::info!(bundle_id = %bundle_id, item_count, "🔍 Query returned items");

        let item = result.items
            .as_ref()
            .and_then(|items| items.first())
            .ok_or_else(|| DynamoDbError::NotFound)?;

        let event_type = item.get("EventType")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing EventType".into()))
            .and_then(|s| s.parse::<EventType>().map_err(|_| DynamoDbError::Deserialization(format!("Invalid EventType: {}", s))))?;

        let bundle_status = item.get("BundleStatus")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.parse::<BundleStatus>())
            .transpose()
            .map_err(|e| DynamoDbError::Deserialization(format!("Invalid BundleStatus: {}", e)))?;

        let transaction_status = item.get("TransactionStatus")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.parse::<TransactionStatus>())
            .transpose()
            .map_err(|e| DynamoDbError::Deserialization(format!("Invalid TransactionStatus: {}", e)))?;

        let leg = item.get("Leg")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.parse::<TransactionLeg>())
            .transpose()
            .map_err(|e| DynamoDbError::Deserialization(format!("Invalid Leg: {}", e)))?;

        let user_id = item.get("UserID")
            .and_then(|v| v.as_s().ok().map(ToOwned::to_owned))
            .unwrap_or_default();

        let created_at = item.get("CreatedAt")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing CreatedAt".into()))
            .and_then(|s| DateTime::parse_from_rfc3339(s).map_err(|e| DynamoDbError::Deserialization(format!("Invalid CreatedAt format: {}", e))))
            .map(|dt| dt.with_timezone(&Utc))?;

        let bundle_json = item.get("BundleSnapshot")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing BundleSnapshot".into()))?;

        let bundle_snapshot: TransactionBundle = serde_json::from_str(bundle_json)
            .map_err(|e| DynamoDbError::Deserialization(e.to_string()))?;

        Ok(TransactionEvent {
            event_id: String::new(), // Not returned from DB, can be ignored here or fetched separately
            bundle_id: bundle_id.to_string(),
            user_id,
            event_type,
            leg,
            bundle_status,
            transaction_status,
            created_at,
            bundle_snapshot,
        })
    }
}
