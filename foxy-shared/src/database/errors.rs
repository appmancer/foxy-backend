use std::fmt;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_cloudwatch::error::BuildError;
use aws_sdk_dynamodb::operation::batch_get_item::BatchGetItemError;
use aws_sdk_dynamodb::operation::query::QueryError;

#[derive(Debug)]
pub enum DynamoDbError {
    MissingEnvVar(std::env::VarError),
    DynamoDbOperation(String),
    CloudWatchOperation(String),
    KeyBuildFailed(String),
    AwsSdkError(String),
    TaskJoinError(String),
    InvalidJSON(String),
    Serialization(String),
    AlreadyPersisted(String),
    Deserialization(String),
    NotFound,
}

impl From<serde_json::Error> for DynamoDbError {
    fn from(err: serde_json::Error) -> Self {
        DynamoDbError::InvalidJSON(format!("{:?}", err))
    }
}

impl From<std::env::VarError> for DynamoDbError {
    fn from(err: std::env::VarError) -> Self {
        DynamoDbError::MissingEnvVar(err)
    }
}

impl From<SdkError<PutItemError>> for DynamoDbError {
    fn from(err: SdkError<PutItemError>) -> Self {
        DynamoDbError::DynamoDbOperation(format!("DynamoDB error: {}", err))
    }
}

impl From<SdkError<BatchGetItemError>> for DynamoDbError {
    fn from(err: SdkError<BatchGetItemError>) -> Self {
        DynamoDbError::DynamoDbOperation(format!("DynamoDB BatchGetItem error: {}", err))
    }
}

impl From<SdkError<QueryError>> for DynamoDbError {
    fn from(err: SdkError<QueryError>) -> Self {
        DynamoDbError::DynamoDbOperation(format!("DynamoDB Query error: {}", err))
    }
}

impl From<aws_sdk_cloudwatch::error::BuildError> for DynamoDbError {
    fn from(err: BuildError) -> Self {
        DynamoDbError::CloudWatchOperation(format!("CloudWatch error: {}", err))
    }
}

impl fmt::Display for DynamoDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DynamoDbError::MissingEnvVar(e) => write!(f, "Missing environment variable: {}", e),
            DynamoDbError::DynamoDbOperation(e) => write!(f, "DynamoDB operation failed: {}", e),
            DynamoDbError::CloudWatchOperation(e) => write!(f, "CloudWatch operation failed: {}", e),
            DynamoDbError::NotFound => write!(f, "DynameoDb operation failed: Data not found"),
            DynamoDbError::KeyBuildFailed(e) => write!(f, "DynameoDb operation failed: Key build failed: {}", e),
            DynamoDbError::AwsSdkError(e) => write!(f, "DynameoDb operation failed: AWS SDK error: {}", e),
            DynamoDbError::TaskJoinError(e) => write!(f, "DynameoDb operation failed: Task join error: {}", e),
            DynamoDbError::InvalidJSON(e) => write!(f, "DynameoDb operation failed: Invalid JSON: {}", e),
            DynamoDbError::Serialization(e) => write!(f, "DynameoDb operation failed: Serialization error: {}", e),
            DynamoDbError::AlreadyPersisted(e) => write!(f, "DynameoDb operation failed: Already persisted error: {}", e),
            DynamoDbError::Deserialization(e) => write!(f, "DynameoDb operation failed: Deserialization error: {}", e),
        }
    }
}
