use aws_sdk_cognitoidentityprovider::{Client as CognitoClient};
use std::collections::HashMap;
use crate::utilities::config;
use aws_sdk_cognitoidentityprovider::types::{AuthFlowType, AuthenticationResultType};
use crate::utilities::token_validation;
use crate::utilities::token_decoding;
use crate::models::errors::ValidateError;
use crate::models::auth::GoogleClaims;

/// Generates authentication tokens for a given user ID (sub)
pub async fn generate_tokens(client: &CognitoClient, sub: &str) -> Result<AuthenticationResultType, ValidateError> {
    let user_pool_id = config::get_user_pool_id();
    let client_id = config::get_user_pool_client_id();

    let mut auth_params = HashMap::new();
    auth_params.insert("USERNAME".to_string(), sub.to_string());

    client
        .admin_initiate_auth()
        .user_pool_id(&user_pool_id)
        .client_id(&client_id)
        .auth_flow(AuthFlowType::CustomAuth)
        .set_auth_parameters(Some(auth_params))
        .send()
        .await
        .map_err(|err| ValidateError::CognitoCheckFailed(format!("Service error: {:?}", err)))
        .and_then(|resp| resp.authentication_result.ok_or_else(|| {
            ValidateError::TokenGenerationFailed("No authentication result".to_string())
        }))
}

pub async fn validate_id_token(id_token: &str, client_id: &str) -> Result<GoogleClaims, ValidateError> {
    let valid_claims = token_validation::validate_google_id_token(id_token, client_id)
        .await
        .map_err(|err| ValidateError::TokenValidationFailed(err.to_string()))?;

    let sub = token_decoding::decode_google_token(id_token)
        .await
        .map(|decoded| decoded.claims.sub)
        .map_err(|err| ValidateError::TokenDecodingFailed(err.to_string()))?;

    // Ensure the `sub` from decoding matches `valid_claims.sub`
    if valid_claims.sub != sub {
        return Err(ValidateError::TokenValidationFailed("Sub mismatch in token".to_string()));
    }

    Ok(valid_claims)
}
