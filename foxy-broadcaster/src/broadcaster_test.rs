#[cfg(test)]
mod broadcaster_tests {
    use crate::test_helpers::*;
    use foxy_shared::database::transaction_event::TransactionEventManager;
    use foxy_shared::models::transactions::{Transaction, TransactionStatus, TokenType, PriorityLevel, Network, TransactionRequest, GasEstimate};
    use foxy_shared::utilities::config;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use chrono::Utc;
    use lambda_runtime::{LambdaEvent, Context};
    use serde_json::Value;
    use crate::broadcast_handler::function_handler_with_cache;
    use std::collections::VecDeque;
    use foxy_shared::services::queue_services::{push_to_broadcast_queue};
    use foxy_shared::utilities::test::{get_dynamodb_client_with_assumed_role, get_sqs_client_with_assumed_role, init_tracing};
    use tracing::info;
    use foxy_shared::models::estimate_flags::EstimateFlags;

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
        let tem = TransactionEventManager::new(
                                                        Arc::new(dynamo_db_client.clone()),
                                                        config::get_transaction_event_table());

        info!("Starting test");
        // Sign transaction
        let signed_tx = sign_test_transaction(&wallet, &provider).await;


        //Its now so complex I can't work out how to create a transaction that will process. I was
        //trying from first principles, and creating a TransactionRequest, but maybe I'm better off
        //trying Transaction::new() and just building it
    }
}
