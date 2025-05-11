use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserDevice {
    pub device_fingerprint: String,
    pub push_token: String,
    pub platform: String,
    pub app_version: String,
}

#[derive(Debug)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
}

#[derive(Deserialize, Debug)]
pub struct ServiceAccountKey {
    pub private_key: String,
    pub client_email: String,
    pub token_uri: String,
}

#[derive(Serialize)]
pub struct FirebaseClaims<'a> {
    pub iss: &'a str,
    pub scope: &'a str,
    pub aud: &'a str,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}
