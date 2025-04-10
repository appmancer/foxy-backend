use jsonwebtoken::{decode, decode_header, Validation, Algorithm, DecodingKey};
use reqwest::Client;
use serde::{Deserialize};
use std::collections::HashMap;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use crate::models::auth::GoogleClaims;
use rsa::pkcs1::EncodeRsaPublicKey;
use serde_json;
use pkcs1::LineEnding;


#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: String,   // Unique user identifier from Google
    pub exp: usize,    // Expiration time (Unix timestamp)
    pub iss: String,   // Issuer (Cognito User Pool URL)
    pub username: String, // The Cognito sub used as the user id
}

async fn fetch_jwks(jwks_url: &str) -> Result<HashMap<String, String>, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(jwks_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch JWKS: {}", e))?;
    let jwks: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JWKS: {}", e))?;

    let mut keys = HashMap::new();
    if let Some(keys_array) = jwks["keys"].as_array() {
        for key in keys_array {
            if let (Some(kid), Some(n), Some(e)) = (
                key["kid"].as_str(),
                key["n"].as_str(),
                key["e"].as_str(),
            ) {
                // Decode n and e as base64url
                let modulus_bytes = URL_SAFE_NO_PAD
                    .decode(n)
                    .map_err(|e| format!("Failed to decode 'n': {}", e))?;
                let exponent_bytes = URL_SAFE_NO_PAD
                    .decode(e)
                    .map_err(|e| format!("Failed to decode 'e': {}", e))?;

                // Create the RSA public key
                let public_key = rsa::RsaPublicKey::new(
                    rsa::BigUint::from_bytes_be(&modulus_bytes),
                    rsa::BigUint::from_bytes_be(&exponent_bytes),
                )
                    .map_err(|e| format!("Failed to construct RSA public key: {}", e))?;

                // Convert to PEM
                let pem = public_key
                    .to_pkcs1_pem(LineEnding::LF)
                    .map_err(|e| format!("Failed to encode RSA key to PEM: {}", e))?;

                keys.insert(kid.to_string(), pem);
            }
        }
    }

    Ok(keys)
}

pub async fn validate_cognito_token(
    token: &str,
    user_pool_id: &str,
    region: &str,
) -> Result<Claims, String> {
    // Cognito issuer URL
    let issuer = format!("https://cognito-idp.{}.amazonaws.com/{}", region, user_pool_id);

    // Decode the token header to extract the `kid` (Key ID)
    let header = decode_header(token).map_err(|e| format!("Invalid token header: {}", e))?;
    let kid = header.kid.ok_or_else(|| "Token header missing 'kid'".to_string())?;

    // Fetch JWKS from Cognito
    let jwks_url = format!("{}/.well-known/jwks.json", issuer);
    let jwks = fetch_jwks(&jwks_url).await?;

    // Find the public key corresponding to the `kid`
    let public_key = jwks.get(&kid).ok_or_else(|| "Key ID not found in JWKS".to_string())?;

    // Decode and validate the token
    let decoding_key = DecodingKey::from_rsa_pem(public_key.as_bytes())
        .map_err(|e| format!("Failed to create decoding key: {}", e))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&issuer]);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| format!("Token validation failed: {}", e))?;

    Ok(token_data.claims)
}

pub async fn validate_google_id_token(token: &str, client_id: &str) -> Result<GoogleClaims, String> {
    let google_keys_url = "https://www.googleapis.com/oauth2/v3/certs";
    let keys_response = Client::new()
        .get(google_keys_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Google keys: {}", e))?;
    let keys: HashMap<String, Vec<HashMap<String, String>>> = keys_response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google keys: {}", e))?;

    let jwks_keys = keys.get("keys").ok_or("No keys found in JWKS response")?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);

    for key in jwks_keys {
        if let Some(n) = key.get("n") {
            if let Some(e) = key.get("e") {
                match DecodingKey::from_rsa_components(n, e) {
                    Ok(decoding_key) => {
                        if let Ok(decoded) = decode::<GoogleClaims>(token, &decoding_key, &validation) {
                            return Ok(decoded.claims);
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to create DecodingKey: {}", err);
                    }
                }
            }
        }
    }

    Err("Token validation failed".to_string())
}
