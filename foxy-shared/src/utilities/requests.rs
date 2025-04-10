use lambda_http::Request;
use serde_json::{json, Value};

/// Extracts the JSON body from a request.
pub fn extract_body(event: &Request) -> Value {
    serde_json::from_slice(event.body().as_ref()).unwrap_or_else(|_| json!({}))
}

/// Extracts the Bearer token from the Authorization header.
pub fn extract_bearer_token(event: &Request) -> Option<&str> {
    event.headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|auth_header| auth_header.strip_prefix("Bearer "))
}
