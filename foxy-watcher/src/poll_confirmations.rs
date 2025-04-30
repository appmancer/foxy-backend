use std::sync::Arc;
use ethers_core::types::H256;
use ethers_providers::{Http, Middleware, Provider};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::errors::AppError;
use foxy_shared::models::transactions::{TransactionStatus, TransactionEvent, TransactionLeg};
use foxy_shared::services::cloudwatch_services::OperationMetricTracker;
use tracing::{error, info};
use foxy_shared::views::status_view::TransactionStatusViewManager;
use crate::errors::WatcherError;

pub async fn poll_confirmations(
    provider: &Arc<Provider<Http>>,
    tem: &Arc<TransactionEventManager>,
    tsm: &Arc<TransactionStatusViewManager>,
) -> Result<u32, WatcherError> {
    let mut count = 0;
    let tracker = OperationMetricTracker::build("WatcherConfirmation").await;

    let pending_views = tsm
        .query_by_transaction_status(TransactionStatus::Pending)
        .await
        .map_err(WatcherError::Transaction)?;

    for view in pending_views {
        info!(?view, "🔍 Inspecting TransactionStatusView");
        info!("PK: {}", view.pk);
        info!("TxHash: {:?}", view.tx_hash);
        info!("Status: {:?}", view.status);
        info!("UserID: {:?}", view.user_id);
        info!("BundleID (raw): {:?}", view.bundle_id);

        let bundle_id = view.bundle_id.clone().unwrap_or_else(|| "<missing>".to_string());
        let tx_hash = match &view.tx_hash {
            Some(tx_hash) => tx_hash,
            None => {
                error!(?view, "⛔ View is missing TxHash");
                return Err(WatcherError::MissingTxHash(bundle_id)
                );
            }
        };

        let latest_event = tem
            .get_latest_event(&bundle_id)
            .await
            .map_err(|e| WatcherError::ReceiptFetchFailure(format!("Failed to load latest event: {}", e)))?;

        if latest_event.bundle_snapshot.main_tx.status == TransactionStatus::Confirmed {
            info!("✅ Already confirmed, skipping");
            continue;
        }

        let parsed_hash = tx_hash
            .parse::<H256>()
            .map_err(|_| WatcherError::InvalidTxHashFormat(tx_hash.clone()))?;

        match provider.get_transaction_receipt(parsed_hash).await {
            Ok(Some(receipt)) => {
                if latest_event.leg != Some(TransactionLeg::Main) {
                    info!("⏭️ Skipping non-main leg: {:?}", latest_event.leg);
                    continue;
                }

                let tx = &latest_event.bundle_snapshot.main_tx;

                let updated_tx = tx
                    .clone()
                    .with_status(TransactionStatus::Confirmed)
                    .with_block_number(receipt.block_number.map(|b| b.as_u64()));

                let confirmed_event = TransactionEvent::on_confirmed(&latest_event, &updated_tx, tem.clone())
                    .await
                    .map_err(|e| WatcherError::InvalidState(format!("on_confirmed failed: {}", e)))?;

                tem.clone().persist(&confirmed_event)
                    .await
                    .map_err(|e| WatcherError::DynamoDb(e))?;

                count += 1;
            }
            Ok(None) => {
                // Still pending, do nothing
                continue;
            }
            Err(err) => {
                return Err(WatcherError::ReceiptFetchFailure(format!(
                    "Error fetching tx receipt: {err}"
                )));
            }
        }
    }

    tracker.track::<(), AppError>(&Ok(()), None).await;
    Ok(count)
}
