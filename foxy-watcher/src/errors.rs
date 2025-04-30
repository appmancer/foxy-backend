use url::ParseError;
use std::io;
use thiserror::Error;
use tracing::dispatcher::SetGlobalDefaultError;
use foxy_shared::database::errors::DynamoDbError;
use foxy_shared::models::errors::TransactionError;

#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("Transaction error: {0}")]
    Transaction(#[from] TransactionError),

    #[error("Database error: {0}")]
    DynamoDb(#[from] DynamoDbError),

    #[error("URL parsing error: {0}")]
    UrlParse(#[from] ParseError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Missing tx hash for bundle {0}")]
    MissingTxHash(String),

    #[error("Failed to parse transaction hash: {0}")]
    InvalidTxHashFormat(String),

    #[error("Receipt retrieval failed: {0}")]
    ReceiptFetchFailure(String),

    #[error("Unexpected state: {0}")]
    InvalidState(String),

    #[error("Cannot init: {0}")]
    InitializationError(String),

}

impl From<SetGlobalDefaultError> for WatcherError {
    fn from(e: SetGlobalDefaultError) -> Self {
        WatcherError::InitializationError(e.to_string())
    }
}
