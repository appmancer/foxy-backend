use reqwest::Client;
use serde_json::json;
use aws_sdk_dynamodb::{Client as DynamoDbClient};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_cognitoidentityprovider::types::AttributeType;
use foxy_shared::services::authentication::generate_tokens;
use dotenv::dotenv;
use std::env;
use aws_sdk_sts::Client as StsClient;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_sts::config::Credentials;
use aws_sdk_sts::types::Credentials as StsCredentials;

const ROLE_ARN: &str = "arn:aws:iam::971422686568:role/Foxy-dev-Cognito-Lambda-ExecutionRole";

async fn assume_role() -> Result<StsCredentials, Box<dyn std::error::Error>> {
    let region_provider = RegionProviderChain::default_provider();
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let sts_client = StsClient::new(&shared_config);

    let assumed_role = sts_client.assume_role()
        .role_arn(ROLE_ARN)
        .role_session_name("IntegrationTestSession")
        .send()
        .await?;

    assumed_role.credentials.ok_or_else(|| "No credentials returned".into())
}

async fn get_cognito_client_with_assumed_role() -> Result<CognitoClient, Box<dyn std::error::Error>> {
    let creds = assume_role().await?;
    let region_provider = RegionProviderChain::default_provider();

    // Directly convert &str to String without `ok_or_else`
    let access_key = creds.access_key_id().to_string();
    let secret_key = creds.secret_access_key().to_string();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "CognitoAssumedRole",
        ))
        .load()
        .await;

    Ok(CognitoClient::new(&shared_config))
}

async fn get_dynamo_client_with_assumed_role() -> Result<DynamoDbClient, Box<dyn std::error::Error>> {
    let creds = assume_role().await?; // ✅ Ensure we use the assumed role credentials
    let region_provider = RegionProviderChain::default_provider();

    let access_key = creds.access_key_id().to_owned();
    let secret_key = creds.secret_access_key().to_owned();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "DynamoAssumedRole",
        ))
        .load()
        .await;

    Ok(DynamoDbClient::new(&shared_config))
}

#[tokio::test]
async fn test_phone_number_integration() -> Result<(), Box<dyn std::error::Error>>{
    let _ = dotenv().is_ok();
    let test_user_id = "112527246877271240195";
    let test_phone_number = "+447911123456";
    let test_country_code = "GB";

    // Step 1: Get Cognito Client
    let cognito_client = get_cognito_client_with_assumed_role().await?;

    // Step 2: Generate access token for test user
    let token_result = generate_tokens(&cognito_client, &test_user_id)
        .await
        .expect("Failed to get test token");

    let access_token = token_result.access_token.expect("Access token missing");

    // Step 3: Send request to /phone/verify
    let client = Client::new();
    let response = client
        .post("http://localhost:9000/phone/verify")
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "number": test_phone_number,
            "countrycode": test_country_code
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 201, "Expected a 201 Created response");

    // Step 4: Verify phone number in Cognito
    let user_attributes = cognito_client
        .admin_get_user()
        .user_pool_id(env::var("COGNITO_USER_POOL_ID").unwrap())
        .username(test_user_id)
        .send()
        .await
        .map(|resp| resp.user_attributes.unwrap_or_else(|| vec![]))
        .expect("Failed to fetch user from Cognito");


    let stored_phone_hash = user_attributes
        .iter()
        .find(|attr| attr.name() == "custom:phone_hash")
        .map(|attr| attr.value().unwrap_or_default());

    assert!(stored_phone_hash.is_some(), "Phone hash not found in Cognito");

    // Step 5: Verify phone number in DynamoDB
    let dynamodb_client = get_dynamo_client_with_assumed_role().await?;
    let table_name = env::var("DYNAMODB_USER_LOOKUP_TABLE_NAME").unwrap();
    let get_item_response = dynamodb_client
        .get_item()
        .table_name(&table_name)
        .key("hashed_phone", AttributeValue::S(stored_phone_hash.clone().unwrap().to_string()))
        .send()
        .await
        .expect("Failed to fetch item from DynamoDB");

    assert!(get_item_response.item.is_some(), "Phone hash not found in DynamoDB");

    // Step 6: Cleanup - Remove phone number from Cognito
    cognito_client
        .admin_update_user_attributes()
        .user_pool_id(env::var("COGNITO_USER_POOL_ID").unwrap())
        .username(test_user_id)
        .user_attributes(
            AttributeType::builder() // ✅ Now correctly using `types::AttributeType`
                .name("custom:phone_hash")
                .value("")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("Failed to clear phone number from Cognito");

    // Step 7: Cleanup - Remove phone number from DynamoDB
    dynamodb_client
        .delete_item()
        .table_name(table_name)
        .key("hashed_phone", AttributeValue::S(stored_phone_hash.clone().unwrap().to_string()))
        .send()
        .await
        .expect("Failed to delete item from DynamoDB");

    Ok(())
}
