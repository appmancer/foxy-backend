use std::sync::Arc;
use std::time::Duration;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use dotenv::dotenv;
use ethers_providers::{Http, Provider};
use foxy_shared::services::cloudwatch_services::OperationMetricTracker;
use foxy_shared::models::transactions::{EventType, Transaction, TransactionStatus};
use foxy_shared::state_machine::transaction_event_factory::{TransactionEvent, TransactionEventFactory};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::errors::AppError;
use tokio::signal;
use tokio::sync::Notify;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use crate::poll_confirmations::poll_confirmations;
use crate::poll_finalizations::poll_finalizations;

mod poll_confirmations;
mod poll_finalizations;
mod watcher_tests;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();

    // Set up structured logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("🚀 Starting Foxy Watcher...");

    let provider = Arc::new(Provider::<Http>::try_from(std::env::var("WATCHER_RPC_URL")?)?);
    let config = aws_config::load_from_env().await;
    let dynamo = Arc::new(DynamoDbClient::new(&config));
    let tem = Arc::new(TransactionEventManager::new(dynamo.clone(), std::env::var("TRANSACTION_TABLE")?));

    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_signal = shutdown_notify.clone();

    let confirm_handle = {
        let provider = provider.clone();
        let tem = tem.clone();
        let shutdown = shutdown_notify.clone();

        tokio::spawn(async move {
            loop {
                let tracker = OperationMetricTracker::build("WatcherConfirmation").await;

                match poll_confirmations(&provider, &tem).await {
                    Ok(count) => info!("🔍 Confirmed {} transactions", count),
                    Err(e) => error!(?e, "Watcher error during confirmation poll"),
                }

                tracker.track::<(), AppError>(&Ok(()), None).await;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {},
                    _ = shutdown.notified() => break,
                }
            }
        })
    };

    let finalize_handle = {
        let provider = provider.clone();
        let tem = tem.clone();
        let shutdown = shutdown_notify.clone();

        tokio::spawn(async move {
            loop {
                let tracker = OperationMetricTracker::build("WatcherFinalizer").await;

                match poll_finalizations(&provider, &tem).await {
                    Ok(count) => info!("🔒 Finalized {} transactions", count),
                    Err(e) => error!(?e, "Watcher error during finalization poll"),
                }

                tracker.track::<(), AppError>(&Ok(()), None).await;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(600)) => {},
                    _ = shutdown.notified() => break,
                }
            }
        })
    };

    // Graceful shutdown
    signal::ctrl_c().await?;
    info!("🛑 Received shutdown signal, terminating...");
    shutdown_signal.notify_waiters();

    let _ = tokio::try_join!(confirm_handle, finalize_handle);

    Ok(())
}
