// this file contains models which are limited in scope to the endpoints, so don't
// really belong in the shared project

use std::fmt;
use serde::{Deserialize, Serialize};
use foxy_shared::models::transactions::UnsignedTransaction;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnsignedTransactionPair {
    pub bundle_id: String,
    pub fee: UnsignedTransaction,
    pub main: UnsignedTransaction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SignedTransactionPayload {
    pub bundle_id: String,
    pub fee_signed_tx: String,   // RLP-encoded or hex string
    pub main_signed_tx: String,  // RLP-encoded or hex string
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SignedTransactionError {
    InvalidPayload(String),
}

impl fmt::Display for SignedTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignedTransactionError::InvalidPayload(msg) => write!(f, "Invalid payload: {}", msg),
        }
    }
}