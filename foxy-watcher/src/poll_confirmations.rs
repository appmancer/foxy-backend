use std::sync::Arc;

use ethers_providers::{Middleware, Provider};
use foxy_shared::services::cloudwatch_services::OperationMetricTracker;
use foxy_shared::models::transactions::{EventType, Transaction, TransactionStatus};
use foxy_shared::state_machine::transaction_event_factory::{TransactionEvent, TransactionEventFactory};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::errors::AppError;
use tracing::{info, error};

pub async fn poll_confirmations<M: Middleware + 'static>(
    provider: &Arc<Provider<M>>,
    tem: &Arc<TransactionEventManager>,
) -> Result<u32, AppError> {
    let mut count = 0;

    // Fetch all transactions that are marked as Pending
    let pending_events = tem.query_by_status(TransactionStatus::Pending).await?;

    for event in pending_events {
        let tx = &event.transaction;

        let tx_hash = match &tx.tx_hash {
            Some(hash) => hash,
            None => {
                error!("Transaction {} has no tx_hash", tx.transaction_id);
                continue;
            }
        };

        let tx_hash = tx_hash.parse()?;

        match provider.get_transaction_receipt(tx_hash).await {
            Ok(Some(receipt)) => {
                let updated = tx
                    .clone()
                    .with_status(TransactionStatus::Confirmed)
                    .with_block_number(receipt.block_number.unwrap().as_u64());

                if let Some(new_event) = TransactionEventFactory::process_event(&event, &updated)? {
                    tem.persist_dual(&new_event).await?;
                    info!("📝 Confirmed tx {} in block {}", tx.transaction_id, receipt.block_number.unwrap());
                    count += 1;
                }
            }
            Ok(None) => {
                // Tx still pending
                continue;
            }
            Err(err) => {
                error!(?err, "Error while fetching tx receipt for {}", tx.transaction_id);
            }
        }
    }

    Ok(count)
}