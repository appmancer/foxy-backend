use std::sync::Arc;
use std::time::Duration;

use aws_sdk_dynamodb::Client as DynamoDbClient;
use dotenv::dotenv;
use ethers_providers::{Http, Provider};
use foxy_shared::services::cloudwatch_services::OperationMetricTracker;
use foxy_shared::models::errors::AppError;
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::utilities::config::{get_rpc_url, get_transaction_event_table, get_transaction_view_table};
use tokio::signal;
use tokio::sync::Notify;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use foxy_shared::views::status_view::TransactionStatusViewManager;
use crate::errors::WatcherError;
use crate::poll_confirmations::poll_confirmations;
use crate::poll_finalizations::poll_finalizations;

mod poll_confirmations;
mod poll_finalizations;
mod watcher_tests;
mod errors;

#[tokio::main]
async fn main() -> Result<(), WatcherError> {
    dotenv().ok();

    // Set up structured logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("🚀 Starting Foxy Watcher...");

    let provider = Arc::new(Provider::<Http>::try_from(get_rpc_url())?);
    let config = aws_config::load_from_env().await;
    let dynamo = Arc::new(DynamoDbClient::new(&config));
    let tem = TransactionEventManager::new(dynamo.clone(), get_transaction_event_table());
    let tsm = Arc::new(TransactionStatusViewManager::new(get_transaction_view_table(), dynamo.clone(), tem.clone()));

    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_signal = shutdown_notify.clone();
    let tem1 = tem.clone();
    let tsm1 = tsm.clone();
    let provider1 = provider.clone();

    let confirm_handle = {
        let shutdown = shutdown_notify.clone();

        tokio::spawn(async move {
            loop {
                let tracker = OperationMetricTracker::build("WatcherConfirmation").await;

                match poll_confirmations(&provider1, &tem1, &tsm1).await {
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

    let tem2 = tem.clone();
    let tsm2 = tsm.clone();
    let provider2 = provider.clone();
    let finalize_handle = {
        let shutdown = shutdown_notify.clone();
        tokio::spawn(async move {
            loop {
                let tracker = OperationMetricTracker::build("WatcherFinalizer").await;

                match poll_finalizations(&provider2, &tem2, &tsm2).await {
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
