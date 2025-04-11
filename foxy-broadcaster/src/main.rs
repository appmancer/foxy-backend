use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use lambda_runtime::{service_fn, tracing, Error, LambdaEvent};
use serde_json::Value;
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::services::queue_services::get_sqs_client;
use foxy_shared::utilities::config;

mod broadcast_handler;
mod broadcaster_test;
mod test_helpers;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    config::init();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let recent_tx_hashes = Arc::new(RwLock::new(VecDeque::with_capacity(10)));
    let sqs_client = Arc::new(get_sqs_client().await.unwrap());
    let dynamo_db_client = Arc::new(get_dynamodb_client().await);
    lambda_runtime::run(service_fn(|event: LambdaEvent<Value>| {
        let recent_tx_hashes = recent_tx_hashes.clone();
        let sqs_client = sqs_client.clone();
        let dynamo_db_client = dynamo_db_client.clone();
        async move {
            broadcast_handler::function_handler_with_cache(event,
                                                           recent_tx_hashes,
                                                           &sqs_client,
                                                           dynamo_db_client).await
        }
    }))
        .await?;

    Ok(())
}