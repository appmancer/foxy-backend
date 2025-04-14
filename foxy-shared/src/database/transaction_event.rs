use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use crate::database::errors::DynamoDbError;
use crate::models::transactions::{EventType, Transaction, TransactionStatus};
use crate::state_machine::transaction_event_factory::{TransactionEvent, TransactionEventFactory};
use crate::utilities::config::get_transaction_view_table;
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
    ) -> Result<(), DynamoDbError> {
        if !event.event_id.is_empty() {
            return Err(DynamoDbError::AlreadyPersisted(format!(
                "Attempted to persist an event that already has event_id: {}",
                event.event_id
            )));
        }

        let item = self.to_dynamo_item(event)?;

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

        if let Err(e) = projector.project(&event.transaction_id).await {
            tracing::error!(?e, "Failed to project status view");
        }

        Ok(())
    }

    fn to_dynamo_item(
        &self,
        event: &TransactionEvent,
    ) -> Result<HashMap<String, AttributeValue>, DynamoDbError> {
        let mut item = HashMap::new();
        let event_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let transaction_json = serde_json::to_string(event.transaction())
            .map_err(|e| DynamoDbError::Serialization(e.to_string()))?;

        item.insert("PK".to_string(), AttributeValue::S(format!("Transaction#{}", event.transaction_id)));
        item.insert("SK".to_string(), AttributeValue::S(format!("Event#{}", timestamp)));
        item.insert("EventID".to_string(), AttributeValue::S(event_id));
        item.insert("TransactionID".to_string(), AttributeValue::S(event.transaction_id.clone()));
        item.insert("UserID".to_string(), AttributeValue::S(event.user_id.clone()));
        item.insert("EventType".to_string(), AttributeValue::S(event.event_type.to_string()));
        item.insert("Status".to_string(), AttributeValue::S(event.status.to_string()));
        item.insert("SenderAddress".to_string(), AttributeValue::S(event.sender_address.clone()));
        item.insert("RecipientAddress".to_string(), AttributeValue::S(event.recipient_address.clone()));
        item.insert("CreatedAt".to_string(), AttributeValue::S(timestamp));
        item.insert("Transaction".to_string(), AttributeValue::S(transaction_json));

        Ok(item)
    }

    pub async fn persist_initial_event(self: Arc<Self>, transaction: &mut Transaction) -> Result<(), DynamoDbError> {
        transaction.transaction_id = Uuid::new_v4().to_string();
        let event = TransactionEventFactory::initial_event(transaction.clone());

        self.persist(&event).await?;
        Ok(())
    }

    pub async fn get_latest_event(
        &self,
        transaction_id: &str,
    ) -> Result<TransactionEvent, DynamoDbError> {
        let result = self.client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression("PK = :pk")
            .expression_attribute_values(":pk", AttributeValue::S(format!("Transaction#{}", transaction_id)))
            .scan_index_forward(false)
            .limit(1)
            .send()
            .await
            .map_err(DynamoDbError::from)?;

        let item = result.items
            .as_ref()
            .and_then(|items| items.first())
            .ok_or_else(|| DynamoDbError::NotFound)?;

        let event_type_str = item.get("EventType")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing EventType".into()))?
            .to_string();

        let event_type = event_type_str
            .parse::<EventType>()
            .map_err(|_| DynamoDbError::Deserialization(format!("Invalid status: {}", event_type_str)))?;

        let status_str = item.get("Status")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing Status".into()))?;

        let status = status_str
            .parse::<TransactionStatus>()
            .map_err(|_| DynamoDbError::Deserialization(format!("Invalid status: {}", status_str)))?;

        let sender_address = item.get("SenderAddress")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "".to_string());

        let recipient_address = item.get("RecipientAddress")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "".to_string());

        let user_id = item.get("UserID")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "".to_string());

        let created_at_str = item.get("CreatedAt")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing CreatedAt".into()))?;

        let created_at = DateTime::parse_from_rfc3339(created_at_str)
            .map_err(|e| DynamoDbError::Deserialization(format!("Invalid CreatedAt format: {}", e)))?
            .with_timezone(&Utc);

        let transaction_json = item.get("Transaction")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DynamoDbError::Deserialization("Missing Transaction field".into()))?;

        let transaction: Transaction = serde_json::from_str(transaction_json)
            .map_err(|e| DynamoDbError::Deserialization(e.to_string()))?;

        Ok(TransactionEvent {
            event_id: String::new(),
            transaction_id: transaction_id.to_string(),
            user_id,
            event_type,
            status,
            sender_address,
            recipient_address,
            transaction,
            created_at
        })
    }
}
