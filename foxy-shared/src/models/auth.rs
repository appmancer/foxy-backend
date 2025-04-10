use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleClaims {
    pub sub: String,       // Unique user ID
    pub iss: String,       // Issuer
    pub aud: String,       // Audience
    pub exp: usize,        // Expiration timestamp
    pub email: String,     // Email address
    pub email_verified: bool, // Email verification status
    pub name: Option<String>, // User's name
    pub picture: Option<String>, // Profile picture URL
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UserProfile {
    pub sub: String,                   // Cognito User ID
    pub email: Option<String>,         // User's email
    #[serde(rename = "custom:phone_hash")]
    pub phone_hash: Option<String>,    // Stored hashed phone number
    #[serde(rename = "custom:wallet_address")]
    pub wallet_address: Option<String>,// User's wallet address
    #[serde(rename = "custom:default_currency")]
    pub currency: Option<String>,      // Display currency
    pub name: Option<String>,    // Whole name
    pub preferred_username: Option<String>, // Username (if set)
    pub created_at: Option<String>,    // User creation timestamp
    pub updated_at: Option<String>,    // Last update timestamp
}
