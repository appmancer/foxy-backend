use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::RwLock;
use lambda_runtime::{tracing};
use lambda_http::{run, service_fn, Request};
use foxy_shared::database::client::get_dynamodb_client;
use foxy_shared::services::queue_services::get_sqs_client;
use foxy_shared::utilities::config;

mod broadcast_handler;
mod broadcaster_test;
mod test_helpers;

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    config::init();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .try_init()
        .unwrap_or_else(|_| eprintln!("🔁 tracing_subscriber already initialized"));

    let recent_tx_hashes = Arc::new(RwLock::new(VecDeque::with_capacity(10)));
    let sqs_client = Arc::new(get_sqs_client().await.unwrap());
    let dynamo_db_client = Arc::new(get_dynamodb_client().await);
    run(service_fn(|event: Request| {
        let recent_tx_hashes = recent_tx_hashes.clone();
        let sqs_client = sqs_client.clone();
        let dynamo_db_client = dynamo_db_client.clone();
        async move {
            broadcast_handler::handle_request(event, recent_tx_hashes, &sqs_client, dynamo_db_client).await
        }
    })).await?;

    Ok(())
}