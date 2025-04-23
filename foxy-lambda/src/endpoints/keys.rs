use std::fmt;
use http::Response;
use lambda_http::{Body, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use foxy_shared::models::errors::AuthorizationError;
use foxy_shared::utilities::requests::extract_bearer_token;
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose, Engine as _};
use foxy_shared::utilities::authentication::with_valid_user;
use foxy_shared::utilities::responses::{error_response, success_response};
use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_secretsmanager::error::{ProvideErrorMetadata, SdkError};
use foxy_shared::services::cloudwatch_services::{create_cloudwatch_client, emit_fatality};
use foxy_shared::utilities::config::get_environment;


#[derive(Debug, Deserialize)]
struct DeriveKeyRequest {
    key_version: String,
}

#[derive(Serialize)]
struct DeriveKeyResponse {
    derived_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeyError {
    InvalidRequest,
    InvalidToken(String),
}

impl std::error::Error for KeyError {}

impl From<AuthorizationError> for KeyError {
    fn from(err: AuthorizationError) -> Self {
        match err {
            AuthorizationError::Unauthorized(msg) => KeyError::InvalidToken(msg),
        }
    }
}
impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::InvalidRequest => write!(f, "Invalid request"),
            KeyError::InvalidToken(msg) => write!(f, "Invalid token: {}", msg),
        }
    }
}

pub async fn handler(event: Request, body: Value) -> Result<Response<Body>, lambda_http::Error> {
    let token = extract_bearer_token(&event);
    let cloudwatch_client = create_cloudwatch_client().await;

    let request: Result<DeriveKeyRequest, KeyError> = serde_json::from_value(body)
        .map_err(|e| {
            log::error!("Deserialization error: {:?}", e); // Log the actual error
            KeyError::InvalidRequest
        });

    match &request {
        Ok(request) =>
            log::info!("Request is valid: {:?}", &request),
        Err(err) =>
            log::info!("Request is invalid: {:?}", err)
    };
    match token {
        None => error_response("Missing authorization token"),
        Some(token) => {
            let secrets_client = SecretsManagerClient::new(&aws_config::load_from_env().await);
            match derive_key(token, &request.unwrap().key_version, &secrets_client, &cloudwatch_client).await {
                Ok(key) => {
                    success_response(DeriveKeyResponse{
                        derived_key: key,
                    })
                }
                Err(err) => error_response(format!("{:?}", err)),
            }
        }
    }
}

async fn derive_key(
    token: &str,
    key_version: &str,
    secrets_client: &SecretsManagerClient,
    cloudwatch_client: &CloudWatchClient,
) -> Result<String, KeyError> {
    with_valid_user(token, |user_id| async move {
        let secret_name = format!("foxy/{}/keys/{}", get_environment(), key_version);

        let secret_result = secrets_client
            .get_secret_value()
            .secret_id(secret_name.clone())
            .send()
            .await;

        let secret = match secret_result {
            Ok(secret) => secret,
            Err(err) => {
                match &err {
                    SdkError::ServiceError(inner) => {
                        let real_error = inner.err(); // Get the real GetSecretValueError

                        log::error!("Service error when fetching secret {}: {:?}", secret_name, real_error);

                        if let Some(code) = real_error.code() {
                            log::error!("AWS error code: {}", code);
                        }
                        if let Some(message) = real_error.message() {
                            log::error!("AWS error message: {}", message);
                        }
                    }
                    SdkError::TimeoutError(_) => {
                        log::error!("Timeout error when fetching secret {}", secret_name);
                    }
                    SdkError::DispatchFailure(e) => {
                        log::error!("Network error when fetching secret {}: {:?}", secret_name, e);
                    }
                    _ => {
                        log::error!("Other SDK error when fetching secret {}: {:?}", secret_name, err);
                    }
                }
                emit_fatality(cloudwatch_client, "SecretsManagerFailure").await;
                return Err(KeyError::InvalidRequest);
            }
        };

        let secret_string = match secret.secret_string() {
            Some(s) => s,
            None => {
                log::error!("Secrets Manager response missing secret_string for {}", secret_name);
                emit_fatality(cloudwatch_client, "SecretsManagerMissingSecretString").await;
                return Err(KeyError::InvalidRequest);
            }
        };

        let json = match serde_json::from_str::<serde_json::Value>(&secret_string) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to parse secret_string JSON: {:?}", e);
                emit_fatality(cloudwatch_client, "SecretsManagerInvalidJson").await;
                return Err(KeyError::InvalidRequest);
            }
        };

        let server_root_key = match json.get("server_root_key").and_then(|v| v.as_str()) {
            Some(key) => key,
            None => {
                log::error!("server_root_key missing from parsed secret_string");
                emit_fatality(cloudwatch_client, "SecretsManagerMissingServerRootKey").await;
                return Err(KeyError::InvalidRequest);
            }
        };

        type HmacSha256 = Hmac<Sha256>;
        let mac = HmacSha256::new_from_slice(server_root_key.as_bytes());
        let mut mac = match mac {
            Ok(mac) => mac,
            Err(e) => {
                log::error!("Failed to create HMAC: {:?}", e);
                emit_fatality(cloudwatch_client, "HmacInitializationFailure").await;
                return Err(KeyError::InvalidRequest);
            }
        };

        mac.update(user_id.as_bytes());
        let result = mac.finalize().into_bytes();

        let derived_key = general_purpose::STANDARD.encode(result);

        Ok(derived_key)
    }).await
}

#[cfg(test)]
mod tests {
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::config;
    use foxy_shared::utilities::test::{get_cloudwatch_client_with_assumed_role, get_cognito_client_with_assumed_role, get_secrets_client_with_assumed_role};
    use crate::endpoints::keys::derive_key;

    fn init_logger() {
        let _ = env_logger::builder()
            .is_test(true)
            .try_init();
    }
    #[tokio::test]
    async fn test_derive_key_success() -> Result<(), Box<dyn std::error::Error>> {
        config::init();
        init_logger();

        let test_user_id = "108298283161988749543"; //Jack
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let cloudwatch_client = get_cloudwatch_client_with_assumed_role().await?;
        let secrets_client = get_secrets_client_with_assumed_role().await?;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        let key_version = "v1";
        let key = derive_key(&access_token, &key_version, &secrets_client, &cloudwatch_client).await?;
        assert_eq!(key.len(), 44);
        Ok(())
    }
}
