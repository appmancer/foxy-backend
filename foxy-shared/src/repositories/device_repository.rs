use crate::models::user_device::UserDevice;
use crate::models::errors::DeviceError;
use async_trait::async_trait;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_dynamodb::types::AttributeValue;

/// Interface
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn get_device(
        &self,
        user_id: &str,
        fingerprint: Option<&str>,
    ) -> Result<Option<UserDevice>, DeviceError>;
}

/// DynamoDB-backed implementation
pub struct DynamoDeviceRepository {
    db: DynamoDbClient,
    table_name: String,
}

impl DynamoDeviceRepository {
    pub fn new(db: DynamoDbClient, table_name: String) -> Self {
        Self { db, table_name }
    }
}

#[async_trait]
impl DeviceRepository for DynamoDeviceRepository {
    async fn get_device(
        &self,
        user_id: &str,
        _fingerprint: Option<&str>,
    ) -> Result<Option<UserDevice>, DeviceError> {
        let pk = format!("User#{}", user_id);
        let sk = "Device"; // TODO: support fingerprint as SK variant later

        let res = self
            .db
            .get_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(pk))
            .key("SK", AttributeValue::S(sk.to_string()))
            .send()
            .await
            .map_err(|e| DeviceError::DynamoDBReadFailed(format!("Failed to fetch device: {}", e)))?;

        if let Some(item) = res.item {
            let device: UserDevice = UserDevice::from_item(item)?;
            Ok(Some(device))
        } else {
            Ok(None)
        }
    }
}
