use jsonwebtoken::{decode, DecodingKey, Validation, TokenData, Algorithm};
use crate::models::auth::GoogleClaims;
use reqwest::Client;
use serde_json::Value;
use crate::utilities::config; // Import the config utility

/// Fetch Google's public keys and decode the token
pub async fn decode_google_token(token: &str) -> Result<TokenData<GoogleClaims>, String> {

    // Initialize dotenv (if not already initialized)
    config::init();
    let client_id = config::get_google_client_id();

    // Step 1: Fetch Google's public keys
    let google_keys_url = "https://www.googleapis.com/oauth2/v3/certs";
    let response = Client::new()
        .get(google_keys_url)
        .send()
        .await
        .map_err(|err| format!("Failed to fetch Google's public keys: {}", err))?;

    let keys: Value = response
        .json()
        .await
        .map_err(|err| format!("Failed to parse Google's public keys: {}", err))?;

    // Step 2: Extract the "kid" from the token header
    let header = jsonwebtoken::decode_header(token)
        .map_err(|err| format!("Failed to decode token header: {}", err))?;
    let kid = header.kid.ok_or("No 'kid' found in token header")?;

    // Step 3: Find the matching public key
    let jwk = keys["keys"]
        .as_array()
        .ok_or("No keys found in Google's JWKS response")?
        .iter()
        .find(|key| key["kid"] == kid)
        .ok_or(format!("No matching key found for kid: {}", kid))?;

    let n = jwk["n"]
        .as_str()
        .ok_or("No 'n' field in Google's public key")?;
    let e = jwk["e"]
        .as_str()
        .ok_or("No 'e' field in Google's public key")?;

    // Step 4: Update validation to support multiple audiences
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[client_id]);

    // Step 5: Decode the token using the matching key
    let decoding_key = DecodingKey::from_rsa_components(n, e)
        .map_err(|err| format!("Failed to create decoding key: {}", err))?;

    decode::<GoogleClaims>(token, &decoding_key, &validation)
        .map_err(|err| format!("Failed to decode token: {}", err))
}
