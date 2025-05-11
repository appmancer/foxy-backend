// foxy-lambda/src/services/user_device_service.rs

use aws_sdk_dynamodb::{Client as DynamoDbClient, Error};
use aws_sdk_dynamodb::types::AttributeValue;
use chrono::Utc;
use crate::database::errors::DynamoDbError;
use crate::models::user_device::UserDevice;

pub struct UserDeviceService {
    client: DynamoDbClient,
    table_name: String,
}

impl UserDeviceService {
    pub fn new(client: DynamoDbClient, table_name: impl Into<String>) -> Self {
        Self {
            client,
            table_name: table_name.into(),
        }
    }

    pub async fn store_device(&self, user_id: &str, device: &UserDevice) -> Result<(), DynamoDbError> {
        let item = std::collections::HashMap::from([
            ("PK".to_string(), AttributeValue::S(format!("User#{}", user_id))),
            ("SK".to_string(), AttributeValue::S(format!("Device#{}", device.device_fingerprint))),
            ("user_id".to_string(), AttributeValue::S(user_id.to_string())),
            ("device_fingerprint".to_string(), AttributeValue::S(device.device_fingerprint.clone())),
            ("push_token".to_string(), AttributeValue::S(device.push_token.clone())),
            ("platform".to_string(), AttributeValue::S(device.platform.clone())),
            ("app_version".to_string(), AttributeValue::S(device.app_version.clone())),
            ("last_updated".to_string(), AttributeValue::S(Utc::now().to_rfc3339())),
        ]);

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        Ok(())
    }

    pub async fn get_device(&self, user_id: &str, fingerprint: &str) -> Result<Option<UserDevice>, Error> {
        let result = self.client
            .get_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(format!("User#{}", user_id)))
            .key("SK", AttributeValue::S(format!("Device#{}", fingerprint)))
            .send()
            .await?;

        if let Some(item) = result.item {
            Ok(Some(UserDevice {
                device_fingerprint: item["device_fingerprint"].as_s().unwrap().to_string(),
                push_token: item["push_token"].as_s().unwrap().to_string(),
                platform: item["platform"].as_s().unwrap().to_string(),
                app_version: item["app_version"].as_s().unwrap().to_string(),
            }))
        } else {
            Ok(None)
        }
    }
}
