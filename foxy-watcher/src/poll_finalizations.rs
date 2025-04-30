use std::sync::Arc;
use ethers_core::types::H256;
use ethers_providers::{Http, Middleware, Provider};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::transactions::{TransactionStatus, TransactionEvent, TransactionLeg};
use tracing::{error, info};
use foxy_shared::views::status_view::TransactionStatusViewManager;
use crate::WatcherError;

pub async fn poll_finalizations(
    provider: &Provider<Http>,
    tem: &Arc<TransactionEventManager>,
    tsm: &Arc<TransactionStatusViewManager>,
) -> Result<u32, WatcherError> {
    let mut count = 0;

    // Load all bundles where status is MainConfirmed (i.e., main_tx is confirmed, fee_tx is next)
    let confirmed_views = tsm.query_by_transaction_status(TransactionStatus::Confirmed).await?;

    for view in confirmed_views {
        let bundle_id = view.bundle_id.clone().unwrap_or_else(|| "<missing>".to_string());
        let Some(tx_hash) = &view.tx_hash else {
            error!(?view, "⛔ View is missing TxHash");
            continue;
        };

        let Ok(latest_event) = tem.get_latest_event(&bundle_id).await else {
            error!(bundle_id = %view.pk, "⚠️ Failed to load latest event");
            continue;
        };

        // Check if this view relates to the fee leg
        if latest_event.leg != Some(TransactionLeg::Fee) {
            continue; // We're only interested in fee confirmations at this stage
        }

        if latest_event.bundle_snapshot.fee_tx.status == TransactionStatus::Confirmed {
            info!("✅ Already finalised, skipping");
            continue;
        }

        let parsed_hash = match tx_hash.parse::<H256>() {
            Ok(h) => h,
            Err(e) => {
                error!(?e, "❌ Invalid tx hash: {}", tx_hash);
                continue;
            }
        };

        // Look up the transaction receipt
        match provider.get_transaction_receipt(parsed_hash).await {
            Ok(Some(receipt)) => {
                let block = receipt.block_number.map(|b| b.as_u64());
                let status = receipt.status.map(|s| s.as_u64());

                // Apply confirmation logic (e.g., status == 1)
                if status != Some(1) {
                    error!(tx_hash = %tx_hash, "❌ Fee tx receipt has failure status: {:?}", status);
                    continue;
                }
                if latest_event.leg != Some(TransactionLeg::Fee) {
                    info!("⏭️ Skipping non-fee leg: {:?}", latest_event.leg);
                    continue;
                }

                let tx = &latest_event.bundle_snapshot.fee_tx;

                // Build new event for fee confirmation
                let updated_tx = tx
                    .clone()
                    .with_status(TransactionStatus::Confirmed)
                    .with_block_number(block)
                    .with_receipt_status(status);

                let confirmed_event = TransactionEvent::on_confirmed(&latest_event, &updated_tx, tem.clone()).await?;
                tem.clone().persist(&confirmed_event).await?;

                info!(tx_hash = %tx_hash, block = ?block, "✅ Finalized fee leg");
                count += 1;
            }
            Ok(None) => {
                continue; // Still pending
            }
            Err(err) => {
                error!(?err, "❌ Error fetching tx receipt");
            }
        }
    }

    Ok(count)
}
