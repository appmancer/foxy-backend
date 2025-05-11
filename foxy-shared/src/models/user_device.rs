// src/models/user_device.rs

use std::collections::HashMap;
use aws_sdk_dynamodb::types::AttributeValue;
use serde::{Deserialize, Serialize};
use crate::models::errors::DeviceError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserDevice {
    pub device_fingerprint: String,
    pub push_token: String,
    pub platform: String,
    pub app_version: String
}

impl UserDevice {
    pub fn new(device_fingerprint: String, push_token: String, platform: String, app_version: String) -> Self {

        Self {
            device_fingerprint,
            push_token,
            platform,
            app_version
        }
    }

    pub fn from_item(item: HashMap<String, AttributeValue>) -> Result<Self, DeviceError> {
        let device_fingerprint = item.get("DeviceFingerprint")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DeviceError::DynamoDBReadFailed("Missing DeviceFingerprint".into()))?
            .to_string();

        let push_token = item.get("PushToken")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DeviceError::DynamoDBReadFailed("Missing PushToken".into()))?
            .to_string();

        let platform = item.get("Platform")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| DeviceError::DynamoDBReadFailed("Missing Platform".into()))?
            .to_string();

        let app_version = item.get("AppVersion")
            .and_then(|v| v.as_s().ok())
            .map_or("1.0".to_string(), |s| s.to_string());

        Ok(UserDevice {
            device_fingerprint,
            push_token,
            platform,
            app_version,
        })
    }
}
