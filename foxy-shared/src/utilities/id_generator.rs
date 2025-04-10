use uuid::Uuid;

/// Generates a unique transaction ID
pub fn generate_transaction_id() -> String {
    format!("tx_{}", Uuid::new_v4())
}
