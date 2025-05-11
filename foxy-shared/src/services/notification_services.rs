use std::fs;
use std::sync::Arc;
use aws_sdk_cloudwatch::types::StandardUnit;
use tokio::sync::RwLock;
use chrono::{Utc, Duration, DateTime};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use crate::models::notifications::{FirebaseClaims, NotificationPayload, ServiceAccountKey, TokenResponse};
use crate::models::user_device::UserDevice;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use crate::models::errors::NotificationError;
use crate::models::transactions::TransactionBundle;
use crate::repositories::device_repository::DeviceRepository;
use crate::services::cloudwatch_services::{create_cloudwatch_client, emit_metric};

// Holds the service account and token cache
pub struct FirebaseClient {
    key: ServiceAccountKey,
    cached_token: Arc<RwLock<Option<(String, DateTime<Utc>)>>>,
    project_id: String,
    cloudwatch: Arc<CloudWatchClient>,
    device_repository: Arc<dyn DeviceRepository>,
}

impl FirebaseClient {
    pub async fn new(path: &str,
                     project_id: &str,
                     device_repository: Arc<dyn DeviceRepository>,) -> Self {
        let key = load_service_account_key(path);
        let cloudwatch = Arc::new(create_cloudwatch_client().await);

        Self {
            key,
            project_id: project_id.to_string(),
            cached_token: Arc::new(RwLock::new(None)),
            cloudwatch,
            device_repository,
        }
    }

    pub async fn notify_transaction_confirmed(
        &self,
        bundle: &TransactionBundle,
    ) -> Result<(), NotificationError> {
        if let Some(metadata) = &bundle.metadata {
            let sender_name = metadata
                .sender
                .as_ref()
                .map(|s| s.name.as_str())
                .unwrap_or("<unknown>");

            let title = "💸 Payment Confirmed";
            let recipient_body = format!(
                "You received £{} from {}",
                metadata.expected_currency_amount,
                sender_name
            );

            if let Some(recipient) = &metadata.recipient {
                let recipient_id = &recipient.user_id;

                if let Err(e) = self.notify_user(recipient_id, title, &recipient_body).await {
                    log::error!("❌ Failed to notify recipient {}: {:?}", recipient_id, e);
                } else {
                    log::info!("📲 Notified recipient {}", recipient_id);
                }
            }

            if let Some(sender) = &metadata.sender {
                let sender_id = &sender.user_id;
                let recipient_name = metadata
                    .recipient
                    .as_ref()
                    .map(|r| r.name.as_str())
                    .unwrap_or("<unknown>");

                let sender_body = format!(
                    "Your payment of £{} to {} has been confirmed",
                    metadata.expected_currency_amount, recipient_name
                );

                if let Err(e) = self.notify_user(sender_id, title, &sender_body).await {
                    log::error!("❌ Failed to notify sender {}: {:?}", sender_id, e);
                } else {
                    log::info!("📲 Notified sender {}", sender_id);
                }
            }
        }

        Ok(())
    }

    pub async fn notify_user(
        &self,
        user_id: &str,
        title: &str,
        body: &str,
    ) -> Result<(), NotificationError> {
        let device_opt = self
            .device_repository
            .get_device(user_id, None)
            .await
            .map_err(|e| NotificationError::DeviceLookupFailed(format!("Device lookup failed for {}: {}", user_id, e)))?;

        let device = match device_opt {
            Some(d) => d,
            None => {
                log::warn!("No device found for user {}, skipping notification", user_id);
                return Ok(()); // not an error
            }
        };

        let payload = NotificationPayload {
            title: title.to_string(),
            body: body.to_string(),
        };

        self.send_to_device(&device, &payload).await
    }

    pub async fn send_to_device(
        &self,
        device: &UserDevice,
        payload: &NotificationPayload,
    ) -> Result<(), NotificationError> {
        let token = self.get_access_token().await?;
        let client = reqwest::Client::new();

        let message = serde_json::json!({
            "message": {
                "token": device.push_token,
                "notification": {
                    "title": payload.title,
                    "body": payload.body
                },
                "android": {
                    "priority": "high"
                }
            }
        });

        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );

        let res = client
            .post(&url)
            .bearer_auth(token)
            .json(&message)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await?;
            log::warn!("Push failed with {}: {}", status, text);
            return Err(NotificationError::FcmPushFailed(format!("Push failed: {}", text)));
        }

        Ok(())
    }


    async fn get_access_token(&self) -> Result<String, NotificationError> {
        let refresh_margin = Duration::minutes(5);
        let now = Utc::now();

        let mut guard = self.cached_token.write().await;
        if let Some((token, expiry)) = guard.as_ref() {
            let seconds_remaining = (*expiry - now).num_seconds();
            if *expiry - refresh_margin > now {
                log::info!(
                "[Push] Firebase token cache hit (expires in {}s)",
                seconds_remaining
            );

                emit_metric(
                    &self.cloudwatch,
                    "FirebaseTokenCacheHits",
                    1.0,
                    StandardUnit::Count,
                ).await;

                return Ok(token.clone());
            } else {
                log::info!(
                "[Push] Firebase token near expiry ({}s remaining) — refreshing",
                seconds_remaining
            );
            }
        } else {
            log::info!("[Push] Firebase token cache miss — no token loaded yet");
        }

        let jwt = create_jwt(&self.key);

        let token_result = exchange_jwt_for_token(&jwt).await;
        let token = match token_result {
            Ok(t) => t,
            Err(e) => {
                emit_metric(
                    &self.cloudwatch,
                    "FirebaseTokenRefreshFailures",
                    1.0,
                    StandardUnit::Count,
                ).await;
                return Err(e);
            }
        };

        let expiry = Utc::now() + Duration::minutes(50);
        *guard = Some((token.clone(), expiry));

        emit_metric(
            &self.cloudwatch,
            "FirebaseTokenCacheMisses",
            1.0,
            StandardUnit::Count,
        ).await;

        log::info!("[Push] New Firebase token cached (valid until {})", expiry);

        Ok(token)
    }

}



fn load_service_account_key(path: &str) -> ServiceAccountKey {
    let data = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Unable to read key file at {}: {}", path, e));

    serde_json::from_str(&data)
        .unwrap_or_else(|e| panic!("Invalid service account JSON in {}: {}", path, e))
}


fn create_jwt(sa: &ServiceAccountKey) -> String {
    let now = Utc::now();
    let claims = FirebaseClaims {
        iss: &sa.client_email,
        scope: "https://www.googleapis.com/auth/firebase.messaging",
        aud: &sa.token_uri,
        iat: now.timestamp(),
        exp: (now + Duration::minutes(60)).timestamp(),
    };

    let key = EncodingKey::from_rsa_pem(sa.private_key.replace("\\n", "\n").as_bytes())
        .expect("Invalid private key format");

    encode(&Header::new(Algorithm::RS256), &claims, &key).expect("JWT creation failed")
}



async fn exchange_jwt_for_token(jwt: &str) -> Result<String, NotificationError> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", jwt),
    ];

    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !res.status().is_success() {
        let body = res.text().await?;
        return Err(NotificationError::TokenExchangeFailed(format!("Token exchange failed: {}", body)));
    }

    let token_response: TokenResponse = res.json().await?;
    Ok(token_response.access_token)
}
