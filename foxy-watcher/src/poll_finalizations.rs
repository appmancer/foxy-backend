use std::sync::Arc;

use ethers_providers::{Middleware, Provider};
use foxy_shared::database::transaction_event::TransactionEventManager;
use foxy_shared::models::transactions::{TransactionStatus, Transaction};
use foxy_shared::state_machine::transaction_event_factory::TransactionEventFactory;
use foxy_shared::models::errors::AppError;
use tracing::{info, error};

/// Placeholder function to simulate finality check.
/// In production, this should call Optimism's L1 batch mapping or use a block explorer API.
async fn is_finalized(_block_number: u64) -> bool {
    // Temporary logic: assume blocks older than 20 minutes are finalized
    // Replace with actual L1 finality check when available
    true
}

pub async fn poll_finalizations<M: Middleware + 'static>(
    provider: &Arc<Provider<M>>,
    tem: &Arc<TransactionEventManager>,
) -> Result<u32, AppError> {
    let mut count = 0;

    // Fetch all transactions that are marked as Confirmed
    let confirmed_events = tem.query_by_status(TransactionStatus::Confirmed).await?;

    for event in confirmed_events {
        let tx = &event.transaction;

        let block_number = match tx.block_number {
            Some(num) => num,
            None => {
                error!("Transaction {} has no block number", tx.transaction_id);
                continue;
            }
        };

        if is_finalized(block_number).await {
            let updated = tx.clone().with_status(TransactionStatus::Finalized);

            if let Some(new_event) = TransactionEventFactory::process_event(&event, &updated)? {
                tem.persist_dual(&new_event).await?;
                info!("🔒 Finalized tx {} in block {}", tx.transaction_id, block_number);
                count += 1;
            }
        }
    }

    Ok(count)
}