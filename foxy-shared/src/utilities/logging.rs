use serde_json::json;
use log::{info, error};

/// Logs an informational event to CloudWatch in JSON format.
pub fn log_info(event: &str, message: &str) {
    info!("{}", json!({
        "event": event,
        "message": message
    }));
}

/// Logs an error event to CloudWatch in JSON format.
pub fn log_error(event: &str, error_message: &str) {
    error!("{}", json!({
        "event": event,
        "error": error_message
    }));
}
