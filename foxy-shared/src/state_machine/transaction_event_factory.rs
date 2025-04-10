use serde::{Deserialize, Serialize};
use chrono::Utc;
use crate::models::transactions::{EventType, Transaction, TransactionStatus};
use crate::models::errors::TransactionError;

/// General event structure for all transaction lifecycle events
#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct TransactionEvent {
    pub event_id: String,
    pub transaction_id: String,
    pub user_id: String, // Cognito sub ID
    pub event_type: EventType,
    pub status: TransactionStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub sender_address: String,
    pub recipient_address: String,
    pub transaction: Transaction,
}

impl TransactionEvent {
    pub fn new(
        transaction_id: String,
        user_id: String,
        event_type: EventType,
        status: TransactionStatus,
        created_at: chrono::DateTime<Utc>,
        transaction: Transaction,
    ) -> Self {
        Self {
            event_id: String::default(), //the db will apply an id
            transaction_id,
            user_id,
            event_type,
            status,
            created_at,
            sender_address: transaction.sender_address.clone(),
            recipient_address: transaction.recipient_address.clone(),
            transaction,
        }
    }

    pub fn transaction(&self) -> &Transaction {
        &self.transaction
    }
}

pub struct TransactionEventFactory;

impl TransactionEventFactory {
    pub fn process_event(
        last_event: &TransactionEvent,
        new_tx: &Transaction,
    ) -> Result<Option<TransactionEvent>, TransactionError> {
        match (&last_event.event_type, &new_tx.status) {
            (EventType::Creation, TransactionStatus::Signed) => Self::created_signed_event(last_event, new_tx),
            (EventType::Broadcasting, TransactionStatus::Broadcasted) => Self::created_broadcast_event(last_event, new_tx),
            _ => Ok(None),
        }
    }

    pub fn initial_event(transaction: Transaction) -> TransactionEvent {
        TransactionEvent::new(
            transaction.transaction_id.clone(),
            transaction.user_id.clone(),
            EventType::Creation,
            TransactionStatus::Created,
            Utc::now(),
            transaction,
        )
    }

    fn created_broadcast_event(
        last_event: &TransactionEvent,
        new_tx: &Transaction,
    ) -> Result<Option<TransactionEvent>, TransactionError> {

        let new_event = TransactionEvent::new(
            new_tx.transaction_id.clone(),
            last_event.user_id.clone(),
            EventType::Broadcasting,
            TransactionStatus::Broadcasted,
            Utc::now(),
            new_tx.clone(),
        );

        Ok(Some(new_event))
    }


    fn created_signed_event(
        last_event: &TransactionEvent,
        new_tx: &Transaction,
    ) -> Result<Option<TransactionEvent>, TransactionError> {

        if new_tx.signed_tx.is_none() {
            return Err(TransactionError::MissingSignatureData(
                "transaction_hash is required for signed state".into(),
            ));
        }

        let new_event = TransactionEvent::new(
            new_tx.transaction_id.clone(),
            last_event.user_id.clone(),
            EventType::Signing,
            TransactionStatus::Signed,
            Utc::now(),
            new_tx.clone(),
        );

        Ok(Some(new_event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::models::transactions::{Transaction, TransactionStatus, TokenType, Network, Metadata, PriorityLevel}; // adjust paths as needed

    fn base_transaction() -> Transaction {
        Transaction {
            transaction_id: "tx123".to_string(),
            user_id: "user123".to_string(),
            sender_address: "0xsender".to_string(),
            recipient_address: "0xrecipient".to_string(),
            transaction_value: 1000,
            token_type: TokenType::ETH,
            status: TransactionStatus::Signed,
            network_fee: 10,
            service_fee: 5,
            total_fees: 15,
            fiat_value: 100,
            fiat_currency: "GBP".to_string(),
            chain_id: 10,
            signed_tx: None,
            transaction_hash: Some("0xhash".to_string()),
            event_log: None,
            metadata: Metadata::default(),
            priority_level: PriorityLevel::Standard,
            network: Network::OptimismSepolia,
            gas_price: None,
            gas_used: None,
            gas_limit: None,
            nonce: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            total_fee_paid: None,
            exchange_rate: None,
            block_number: None,
            receipt_status: None,
            contract_address: None,
            approval_tx_hash: None,
            recipient_tx_hash: None,
            fee_tx_hash: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_initial_event_creation() {
        let tx = base_transaction();
        let event = TransactionEventFactory::initial_event(tx.clone());

        assert_eq!(event.transaction_id, tx.transaction_id);
        assert_eq!(event.event_type, EventType::Creation);
        assert_eq!(event.status, TransactionStatus::Created);
        assert_eq!(event.transaction().sender_address, tx.sender_address);
    }

    fn creation_event() -> TransactionEvent {
        TransactionEvent::new(
            "transid".to_string(),
            "userid".to_string(),
            EventType::Creation,
            TransactionStatus::Created,
            Utc::now(),
            base_transaction(),
        )
    }

    #[tokio::test]
    async fn test_handle_creation_to_signed_success() {
        let last_event = creation_event();
        let mut new_tx = base_transaction();
        new_tx.status = TransactionStatus::Signed;
        new_tx.signed_tx = Some("0xDEADBEEF".to_string());

        let result = TransactionEventFactory::created_signed_event(&last_event, &new_tx);

        assert!(result.is_ok());
        let event = result.unwrap().unwrap();
        assert_eq!(event.status, TransactionStatus::Signed);
        assert_eq!(event.event_type, EventType::Signing);
    }

    #[tokio::test]
    async fn test_handle_creation_to_signed_missing_signature_data() {
        let last_event = creation_event();
        let mut new_tx = base_transaction();
        new_tx.status = TransactionStatus::Signed;

        let result = TransactionEventFactory::created_signed_event(&last_event, &new_tx);

        assert!(matches!(result, Err(TransactionError::MissingSignatureData(_))));
    }

    #[tokio::test]
    async fn test_handle_creation_to_signed_wrong_status() {
        let last_event = creation_event();
        let mut new_tx = base_transaction();
        new_tx.status = TransactionStatus::Broadcasted;

        let result = TransactionEventFactory::created_signed_event(&last_event, &new_tx);

        assert!(matches!(result, Err(TransactionError::InvalidStateTransition { .. })));
    }
}

