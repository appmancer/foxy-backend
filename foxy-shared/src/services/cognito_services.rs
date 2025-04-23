use aws_sdk_cognitoidentityprovider::types::{AttributeType, MessageActionType};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use serde_json::Value;
use crate::models::auth::UserProfile;
use crate::models::errors::{CognitoError, ValidateError};
use crate::utilities::{config, security, fields::cognito};
use std::iter::*;
use crate::database::dynamo_identity::get_user_id_from_wallet_address;
use crate::models::transactions::PartyDetails;
use crate::services::cloudwatch_services::OperationMetricTracker;
use crate::track_ok;

pub async fn get_cognito_client() -> CognitoClient {
    let config = aws_config::load_from_env().await;
    CognitoClient::new(&config)
}

pub async fn check_user_exists(client: &CognitoClient, user_id: &str) -> Result<bool, ValidateError> {
    let user_pool_id = config::get_user_pool_id();
    match client
        .admin_get_user()
        .user_pool_id(user_pool_id)
        .username(user_id)
        .send()
        .await
    {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub async fn create_user(client: &CognitoClient, user_id: &str, name: &str, email: &str, phone_number: Option<&str>) -> Result<(), ValidateError> {
    let user_pool_id = config::get_user_pool_id();

    let mut attributes = vec![
        AttributeType::builder().name(cognito::NAME_FIELD).value(name).build(),
        AttributeType::builder().name(cognito::EMAIL_FIELD).value(email).build(),
        AttributeType::builder().name(cognito::DEFAULT_CURRENCY).value("GBP").build(),
    ];

    if let Some(phone) = phone_number {
        attributes.push(AttributeType::builder().name(cognito::PHONE_FIELD).value(phone).build());
    }

    let attributes: Vec<AttributeType> = attributes
        .into_iter()
        .map(|attr| attr.map_err(|e| ValidateError::CognitoCheckFailed(format!("Failed to build attribute: {}", e))))
        .collect::<Result<Vec<_>, _>>()?;

    client
        .admin_create_user()
        .user_pool_id(user_pool_id)
        .username(user_id)
        .message_action(MessageActionType::Suppress)
        .set_user_attributes(Some(attributes))
        .send()
        .await
        .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to create user: {:?}", err)))?;

    Ok(())
}

pub async fn set_permanent_password(client: &CognitoClient, user_id: &str) -> Result<(), ValidateError> {
    let user_pool_id = config::get_user_pool_id();
    let password = security::generate_secure_password();

    client
        .admin_set_user_password()
        .user_pool_id(user_pool_id)
        .username(user_id)
        .password(&password)
        .permanent(true)
        .send()
        .await
        .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to set permanent password: {:?}", err)))?;

    Ok(())
}

pub async fn get_user_display_name_from_wallet(client: &CognitoClient, wallet: &str) -> Result<String, CognitoError> {
    let user_pool_id = config::get_user_pool_id();

    // This assumes you’ve stored wallet addresses in the Cognito `custom:wallet_address` attribute
    let result = client
        .list_users()
        .user_pool_id(user_pool_id)
        .filter(format!("custom:wallet_address = \"{}\"", wallet))
        .send()
        .await?;

    let user = result.users().get(0).ok_or(CognitoError::UserNotFound)?;
    let name_attr = user
        .attributes()
        .iter()
        .find(|attr| attr.name() == "name")
        .map(|attr| attr.value().unwrap_or_default())
        .unwrap_or_default()
        .to_string();

    Ok(name_attr)
}

pub async fn get_party_details_from_wallet(
    client: &CognitoClient,
    dynamo_client: &DynamoDbClient,
    wallet: &str,
) -> Result<PartyDetails, CognitoError> {

    // Step 1: Lookup user ID from wallet address
    let user_id = get_user_id_from_wallet_address(dynamo_client, wallet)
        .await
        .map_err(|_| CognitoError::UserNotFound)?;

    // Step 2: Fetch user profile from Cognito using user ID
    let user_profile = get_user_data(client, &user_id)
        .await
        .map_err(|_| CognitoError::UserNotFound)?;

    // Step 3: Return PartyDetails with user name and wallet
    Ok(PartyDetails {
        name: user_profile.name.unwrap_or_default(),
        wallet: wallet.to_string(),
    })
}

pub async fn get_user_data(client: &CognitoClient, sub: &str) -> Result<UserProfile, String> {
    let user_pool_id = config::get_user_pool_id();

    let response = client
        .admin_get_user()
        .user_pool_id(user_pool_id)
        .username(sub)
        .send()
        .await
        .map_err(|e| format!("Failed to get user: {:?}", e))?;

    let mut user_data = serde_json::Map::new();

    for attr in response.user_attributes.unwrap_or_default() {
        user_data.insert(attr.name, Value::String(attr.value.unwrap_or_default()));
    }

    serde_json::from_value(Value::Object(user_data))
        .map_err(|e| format!("Failed to deserialize user data: {}", e))
}

pub async fn update_user_wallet_address(client: &CognitoClient, user_id: &str, wallet_address: &str) -> Result<(), ValidateError> {
    let user_pool_id = config::get_user_pool_id();

    let wallet_attribute = AttributeType::builder()
        .name(cognito::WALLET_FIELD)
        .value(wallet_address)
        .build()
        .map_err(|e| ValidateError::CognitoCheckFailed(format!("Failed to build attribute: {}", e)))?;

    client
        .admin_update_user_attributes()
        .user_pool_id(user_pool_id)
        .username(user_id)
        .user_attributes(wallet_attribute)
        .send()
        .await
        .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to update wallet address: {:?}", err)))?;

    Ok(())
}

pub async fn update_user_phone_number(client: &CognitoClient, user_id: &str, phone_hash: &str) -> Result<(), ValidateError> {
    let user_pool_id = config::get_user_pool_id();

    let phone_attribute = AttributeType::builder()
        .name(cognito::PHONE_FIELD)
        .value(phone_hash)
        .build()
        .map_err(|e| ValidateError::CognitoCheckFailed(format!("Failed to build attribute: {}", e)))?;

    client
        .admin_update_user_attributes()
        .user_pool_id(user_pool_id)
        .username(user_id)
        .user_attributes(phone_attribute)
        .send()
        .await
        .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to update phone number: {:?}", err)))?;

    Ok(())
}

pub async fn create_user_and_set_password(
    client: &CognitoClient,
    user_id: &str,
    email: Option<&str>,
    name: &str,
    phone_number: Option<&str>,
) -> Result<(), ValidateError> {

    let tracker = OperationMetricTracker::build("CreateUser").await;

    track_ok!(tracker, async {
        // Create the user with attributes
        create_user(client, user_id, name, email.unwrap_or(""), phone_number)
            .await
            .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to create user: {}", err)))?;

        // Set the permanent password
        set_permanent_password(client, user_id)
            .await
            .map_err(|err| ValidateError::CognitoCheckFailed(format!("Failed to set password: {}", err)))?;

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use crate::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};
    use super::*;

    #[tokio::test]
    async fn test_get_user_data_returns_expected_fields() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv::dotenv(); // Load .env
        let client = get_cognito_client_with_assumed_role().await?;

        // Replace with a valid Cognito sub (you can get this from the user pool console)
        let test_id = "112527246877271240195";
        match get_user_data(&client, test_id).await {
            Ok(profile) => {
                println!("✅ Got user profile: {:?}", profile);
                // You can assert on expected fields if you know them

                assert_eq!(profile.sub, "70cca94c-2031-7049-e5ec-e344d01b937e", "Sub should match requested");
            }
            Err(err) => {
                panic!("❌ Failed to fetch user profile: {}", err);
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn get_party_details_from_wallet_returns_expected_details() -> Result<(), Box<dyn std::error::Error>> {
        // Load environment config
        let _ = dotenv().is_ok();

        // Known test wallet with user profile
        let wallet = "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0";

        // Create Cognito client
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let dynamo_db_client = get_dynamodb_client_with_assumed_role().await;

        // Call the function
        let result = get_party_details_from_wallet(&cognito_client, &dynamo_db_client, wallet).await;

        match result {
            Ok(party) => {
                println!("✅ PartyDetails: {:?}", party);
                assert_eq!(party.wallet, wallet);
                assert!(!party.name.is_empty(), "Expected non-empty name");
            }
            Err(err) => {
                panic!("❌ Failed to get PartyDetails: {:?}", err);
            }
        }

        Ok(())
    }


}
