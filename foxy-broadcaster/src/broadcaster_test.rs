#[cfg(test)]
mod broadcaster_tests {
    use crate::test_helpers::*;
    use foxy_shared::database::transaction_event::TransactionEventManager;
    use foxy_shared::models::transactions::{Transaction, TransactionStatus, TokenType, PriorityLevel, Network};
    use foxy_shared::utilities::config;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use chrono::Utc;
    use lambda_runtime::{LambdaEvent, Context};
    use serde_json::Value;
    use crate::broadcast_handler::function_handler_with_cache;
    use std::collections::VecDeque;
    use foxy_shared::services::queue_services::{push_to_broadcast_queue};
    use foxy_shared::utilities::test::{get_dynamodb_client_with_assumed_role, get_sqs_client_with_assumed_role};
    use once_cell::sync::OnceCell;
    use tracing::info;
    use tracing_subscriber::{FmtSubscriber, EnvFilter};
    use foxy_shared::state_machine::transaction_event_factory::TransactionEventFactory;

    static INIT: OnceCell<()> = OnceCell::new();

    fn init_tracing() {
        INIT.get_or_init(|| {
            let subscriber = FmtSubscriber::builder()
                .with_env_filter(EnvFilter::from_default_env()) // optionally set RUST_LOG
                .with_test_writer() // required to capture test output
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set global tracing subscriber");
        });
    }

    #[tokio::test]
    async fn test_broadcaster_end_to_end() {
        config::init();
        init_tracing();
        let _ = env_logger::builder().is_test(true).try_init();
        // Setup wallet & provider
        let wallet = funded_sender_wallet();
        let provider = test_provider();
        let sqs_client = get_sqs_client_with_assumed_role().await.unwrap();
        let queue_url = config::get_broadcast_queue();
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;
        let tem = TransactionEventManager::new(Arc::new(dynamo_db_client.clone()),
                                               config::get_transaction_event_table());

        info!("Starting test");
        // Sign transaction
        let signed_tx = sign_test_transaction(&wallet, &provider).await;

        // Prepare transaction for DynamoDB
        let mut transaction = Transaction {
            transaction_id: "".into(), // Set by persist_initial_event
            user_id: TEST_USER_ID.into(),
            sender_address: TEST_SENDER_ADDRESS.into(),
            recipient_address: TEST_RECIPIENT_ADDRESS.into(),
            transaction_value: 1_000_000_000_000u128,
            token_type: TokenType::ETH,
            status: TransactionStatus::Created,
            network_fee: 21_000,
            service_fee: 0,
            total_fees: 21_000,
            fiat_value: 1,
            fiat_currency: "GBP".into(),
            chain_id: 11155420u64,
            signed_tx: Some(format!("0x{}", hex::encode(signed_tx))),
            transaction_hash: None,
            event_log: None,
            metadata: Default::default(),
            priority_level: PriorityLevel::Standard,
            network: Network::OptimismSepolia,
            gas_price: Some(1_000_000),
            gas_used: None,
            gas_limit: Some(21_000),
            nonce: None,
            max_fee_per_gas: Some(1_000_000),
            max_priority_fee_per_gas: Some(150_000),
            total_fee_paid: None,
            exchange_rate: None,
            block_number: None,
            receipt_status: None,
            contract_address: None,
            approval_tx_hash: None,
            recipient_tx_hash: None,
            fee_tx_hash: None,
            created_at: Utc::now(),
        };

        // Store the transaction/event in DynamoDB
        tem.persist_initial_event(&mut transaction).await.expect("Persist failed");
        let transaction_id = transaction.transaction_id.clone();

        //Now we need to get the event from storage, and let it go through the signing procees
        let event_created = tem.get_latest_event(&transaction_id).await.unwrap();
        let updated_tx = event_created.transaction
            .clone()
            .with_status(TransactionStatus::Signed);

        if let Some(event_created) = TransactionEventFactory::process_event(&event_created, &updated_tx).unwrap_or(None) {
            tem.persist_dual(&event_created).await.ok();
            info!("📝 Broadcast event persisted");
        }
        // Push correct message onto SQS queue
        push_to_broadcast_queue(&sqs_client, &queue_url, &transaction_id, TEST_USER_ID).await.expect("Failed to send message");

        let dynamo_db_client = Arc::new(get_dynamodb_client_with_assumed_role().await);
        // Invoke broadcaster function
        let recent_tx_hashes = Arc::new(RwLock::new(VecDeque::new()));
        function_handler_with_cache(
            LambdaEvent::new(Value::default(), Context::default()),
            recent_tx_hashes.clone(),
            &Arc::new(sqs_client),
            dynamo_db_client.clone(),
        ).await.expect("Handler failed");

        // Check DynamoDB status updated correctly
        let event_after = tem.get_latest_event(&transaction_id).await.unwrap();
        assert_eq!(event_after.status, TransactionStatus::Broadcasted);
    }
}
