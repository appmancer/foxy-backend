use aws_sdk_dynamodb::Client as DynamoDbClient;
pub async fn get_dynamodb_client() -> DynamoDbClient {
    let config = aws_config::load_from_env().await;
    DynamoDbClient::new(&config)
}
