use aws_sdk_dynamodb::types::{AttributeValue, Select};
use aws_sdk_dynamodb::{Client as DynamoDbClient, Client};
use std::env;
use crate::database::errors::DynamoDbError;
use crate::utilities::logging::{log_error, log_info};
use aws_sdk_dynamodb::types::KeysAndAttributes;
use std::collections::{HashMap, HashSet};
use tokio::task;
use futures::future::join_all;
use crate::utilities::config::get_user_lookup_table;
use crate::utilities::fields::{cognito, dynamodb};

pub async fn update_phone_hash(
    dynamodb_client: &DynamoDbClient,
    hashed_phone: &str,
    user_sub: &str,
    wallet_address: &str,
) -> Result<(), DynamoDbError> {
    let table_name = env::var("DYNAMODB_USER_LOOKUP_TABLE_NAME")
        .map_err(|e| {
            log_error("DynamoDB", &format!("MissingEnvVar: {:?}", e)); // ✅ Log missing env variable
            DynamoDbError::MissingEnvVar(e)
        })?;

    log_info("DynamoDB", &format!("Preparing to write hashed_phone={} for user_id={}", hashed_phone, user_sub)); // ✅ Log input data

    match dynamodb_client
        .put_item()
        .table_name(&table_name)
        .item(dynamodb::PHONE_FIELD, AttributeValue::S(hashed_phone.to_string()))
        .item(dynamodb::USER_ID_FIELD, AttributeValue::S(user_sub.to_string()))
        .item(cognito::WALLET_FIELD, AttributeValue::S(wallet_address.to_string()))
        .send()
        .await
    {
        Ok(_) => {
            log_info("DynamoDB", "Successfully updated phone hash in table");
            Ok(())
        }
        Err(err) => {
            log_error("DynamoDB", &format!("Failed to update phone hash: {:?}", err));
            Err(DynamoDbError::from(err))
        }
    }
}

pub async fn parallel_batches(
    client: &Client,
    hashed_phones: Vec<String>,
) -> Result<HashMap<String, String>, DynamoDbError> {

    let mut seen = HashSet::new();
    let unique_hashed_phones: Vec<String> = hashed_phones
        .into_iter()
        .filter(|h| seen.insert(h.clone()))
        .collect();

    let chunk_size = 100;
    let batches = unique_hashed_phones.chunks(chunk_size).enumerate().map(|(i, chunk)| {
        let client = client.clone();
        let chunk_vec = chunk.to_vec();

        log::debug!("Spawning batch {} with {} keys", i, chunk_vec.len());
        for (j, key) in chunk_vec.iter().enumerate() {
            log::debug!("Batch {} - Key[{}]: {}", i, j, key);
        }

        task::spawn(async move {
            let result = batch_lookup(&client, chunk_vec).await;
            if let Err(ref e) = result {
                log::error!("Batch {} failed with error: {:?}", i, e);
            }
            result
        })
    });

    let results = join_all(batches).await;

    let mut final_map = HashMap::new();
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(map)) => {
                log::debug!("Batch {} succeeded with {} results", i, map.len());
                final_map.extend(map);
            }
            Ok(Err(err)) => {
                log::error!("Batch {} failed during DynamoDB lookup: {:?}", i, err);
                return Err(err); // You can `continue` if partial success is acceptable
            }
            Err(join_err) => {
                log::error!("Batch {} panicked or join failed: {:?}", i, join_err);
                return Err(DynamoDbError::TaskJoinError(join_err.to_string()));
            }
        }
    }

    Ok(final_map)
}

pub async fn batch_lookup(client: &Client, hashed_phones: Vec<String>) -> Result<HashMap<String, String>, DynamoDbError> {
    let table_name = get_user_lookup_table();

    log::debug!("Performing batch lookup in table: {}", table_name);

    let keys: Vec<HashMap<String, AttributeValue>> = hashed_phones
        .into_iter()
        .map(|hash| {
            let mut key_map = HashMap::new();
            key_map.insert("hashed_phone".to_string(), AttributeValue::S(hash));
            key_map
        })
        .collect();

    let keys_and_attributes = KeysAndAttributes::builder()
        .set_keys(Some(keys.clone()))
        .projection_expression("#hp, #wa")
        .expression_attribute_names("#hp", "hashed_phone")
        .expression_attribute_names("#wa", "wallet_address")
        .build()
        .map_err(|e| {
            log::error!("Failed to build KeysAndAttributes: {:?}", e);
            DynamoDbError::KeyBuildFailed(e.to_string())
        })?;

    let mut request_items = HashMap::new();
    request_items.insert(table_name.clone(), keys_and_attributes);

    let response = client
        .batch_get_item()
        .set_request_items(Some(request_items))
        .send()
        .await
        .map_err(|err| {
            log::error!("DynamoDB BatchGetItem call failed: {:?}", err);
            DynamoDbError::AwsSdkError(err.to_string())
        })?;

    let mut result_map = HashMap::new();
    if let Some(responses) = response.responses {
        if let Some(items) = responses.get(&table_name) {
            for item in items {
                let maybe_pair = (
                    item.get("hashed_phone").and_then(|v| v.as_s().ok()),
                    item.get("wallet_address").and_then(|v| v.as_s().ok()),
                );

                if let (Some(hash), Some(wallet)) = maybe_pair {
                    result_map.insert(hash.to_string(), wallet.to_string());
                } else {
                    log::warn!("Malformed item returned: {:?}", item);
                }
            }
        }
    } else {
        log::warn!("No response found for table: {}", table_name);
    }

    Ok(result_map)
}


pub async fn get_user_id_from_wallet_address(
    client: &Client,
    wallet_address: &str,
) -> Result<String, DynamoDbError> {
    let table_name = get_user_lookup_table();

    let response = client
        .scan()
        .table_name(table_name)
        .filter_expression("wallet_address = :wallet")
        .expression_attribute_values(":wallet", AttributeValue::S(wallet_address.to_string()))
        .select(Select::AllAttributes)
        .send()
        .await
        .map_err(|e| DynamoDbError::DynamoDbOperation(format!("Scan failed: {}", e)))?;

    let items = response.items();
    if let Some(item) = items.first() {
        if let Some(AttributeValue::S(user_id)) = item.get("user_id") {
            return Ok(user_id.clone());
        }
    }

    Err(DynamoDbError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use tokio;
    use crate::utilities::test::get_dynamodb_client_with_assumed_role;

    #[tokio::test]
    async fn test_get_user_id_from_wallet_address_returns_expected_user_id() {
        let _ = dotenv().is_ok();
        let client = get_dynamodb_client_with_assumed_role().await;

        let wallet_address = "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0";
        let expected_user_id = "112527246877271240195";

        match get_user_id_from_wallet_address(&client, wallet_address).await {
            Ok(user_id) => {
                println!("✅ Retrieved user_id: {}", user_id);
                assert_eq!(user_id, expected_user_id);
            }
            Err(e) => panic!("❌ Failed to get user_id from wallet: {:?}", e),
        }
    }
}