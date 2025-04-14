use std::fmt;
use std::fmt::{Debug};
use phonenumber::ParseError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use reqwest::Error as ReqwestError;
use std::io;
use aws_sdk_cognitoidentityprovider::error::SdkError;
use aws_sdk_cognitoidentityprovider::operation::list_users::ListUsersError;
use aws_sdk_sts::config::http::HttpResponse;
use crate::database::errors::DynamoDbError;
use aws_sdk_dynamodb::error::SdkError as DynamoError;
use ethers_providers::ProviderError;
use serde_json::Error as SerdeJsonError;

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthorizationError {
    Unauthorized(String),
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthorizationError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
        }
    }
}

impl std::error::Error for AuthorizationError {}



#[derive(Debug, Serialize, Deserialize)]
pub enum AuthenticationError {
    Unauthenticated(String),
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthenticationError::Unauthenticated(msg) => write!(f, "Unauthenticated: {}", msg),
        }
    }
}

impl std::error::Error for AuthenticationError {}

#[derive(Debug, Serialize, Deserialize)]
pub enum CognitoError {
    UserNotFound,
}

impl From<SdkError<ListUsersError, HttpResponse>> for CognitoError {
    fn from(_: SdkError<ListUsersError, HttpResponse>) -> Self {
        CognitoError::UserNotFound
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionError {
    // Validation Errors
    InvalidAmount,                      // Amount must be greater than zero
    InvalidToken(String),                        // Token is unsupported
    InvalidPriorityLevel,                // Priority level is not recognized
    InvalidNetwork,                      // The selected network is not supported
    InvalidAddress,                      // From/To address is malformed
    SameSenderReceiver,                  // Sender and recipient addresses must be different
    MessageTooLong,                       // Metadata message exceeds allowed length
    MissingSignatureData(String),
    IncorrectProcess(String),                // Invalid event combination
    InvalidStateTransition { event: String, status: String },
    MissingGasEstimate,
    InvalidExchangeRate,
    InvalidTransactionValue,
    InvalidNetworkFee,
    InvalidServiceFee,

    // System & Processing Errors
    GasPriceUnavailable(String),                  // Could not fetch gas price
    NonceUnavailable,                      // Could not fetch nonce
    BlockchainError(String),               // Generic blockchain-related error (e.g., RPC failure)
    TransactionFailed(String),             // Generic transaction failure
    DatabaseError(String),                 // Failure storing or retrieving transaction data
    Unauthorized,                           // User is not authorized for this action
    StateMachine(String),

    // External Dependencies
    RateLimitExceeded,                      // API rate limits from third-party services
    NetworkIssue,                           // Network failure preventing transaction processing
    InvalidRequest,
    ExchangeRateError(String),
    QueueError(String),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Validation Errors
            TransactionError::InvalidAmount => write!(f, "Transaction amount must be greater than zero."),
            TransactionError::InvalidToken(msg) => write!(f, "Unsupported token: {}", msg),
            TransactionError::InvalidPriorityLevel => write!(f, "Invalid priority level."),
            TransactionError::InvalidNetwork => write!(f, "Unsupported network."),
            TransactionError::InvalidAddress => write!(f, "Invalid sender or recipient address."),
            TransactionError::SameSenderReceiver => write!(f, "Sender and recipient addresses must be different."),
            TransactionError::MessageTooLong => write!(f, "Metadata message exceeds allowed length."),
            TransactionError::MissingGasEstimate => write!(f, "Gas estimate is missing."),
            TransactionError::InvalidStateTransition { event, status } =>
                {write!(f, "Invalid transition from event '{}'' to status '{}'", event, status) },
            TransactionError::InvalidExchangeRate => write!(f, "Invalid exchange rate."),
            TransactionError::InvalidTransactionValue => write!(f, "Invalid transaction value."),
            TransactionError::InvalidNetworkFee => write!(f, "Invalid network fee."),
            TransactionError::InvalidServiceFee => write!(f, "Invalid service fee."),

            // System & Processing Errors
            TransactionError::GasPriceUnavailable(msg) => write!(f, "Could not fetch gas price: {}", msg),
            TransactionError::NonceUnavailable => write!(f, "Could not fetch nonce."),
            TransactionError::BlockchainError(msg) => write!(f, "Blockchain error: {}", msg),
            TransactionError::TransactionFailed(msg) => write!(f, "Transaction failed: {}", msg),
            TransactionError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            TransactionError::Unauthorized => write!(f, "Unauthorized request."),
            TransactionError::StateMachine(msg) => write!(f, "State machine error: {}", msg),

            // External Dependencies
            TransactionError::RateLimitExceeded => write!(f, "Rate limit exceeded. Please try again later."),
            TransactionError::NetworkIssue => write!(f, "Network connectivity issue."),
            TransactionError::InvalidRequest => write!(f, "Invalid request."),
            TransactionError::ExchangeRateError(msg) => write!(f, "Exchange Rate error: {}", msg),
            TransactionError::MissingSignatureData(msg) => write!(f, "Missing signature data: {}", msg),
            TransactionError::IncorrectProcess(msg) => write!(f, "Incorrect process: {}", msg),
            TransactionError::QueueError(msg) => write!(f, "Queue error: {}", msg),
        }
    }
}

impl From<AuthorizationError> for TransactionError {
    fn from(err: AuthorizationError) -> Self {
        match err {
            AuthorizationError::Unauthorized(msg) => TransactionError::InvalidToken(msg),
        }
    }
}

impl From<FetchRateError> for TransactionError {
    fn from(err: FetchRateError) -> Self {
        match err {
            FetchRateError::RequestError(_) => TransactionError::InvalidRequest,
            FetchRateError::IoError(_) => TransactionError::NetworkIssue,
            FetchRateError::MissingRate => TransactionError::ExchangeRateError("Exchange rate missing".to_string()),
        }
    }
}

impl From<DynamoDbError> for TransactionError {
    fn from(err: DynamoDbError) -> Self {
        TransactionError::DatabaseError(format!("{:?}", err))
    }
}

impl From<CognitoError> for TransactionError {
    fn from(err: CognitoError) -> Self {
        TransactionError::DatabaseError(format!("{:?}", err))
    }
}

impl From<aws_sdk_sqs::Error> for TransactionError {
    fn from(err: aws_sdk_sqs::Error) -> Self {
        TransactionError::QueueError(format!("{:?}", err))
    }
}

impl From<NonceError> for TransactionError {
    fn from(err: NonceError) -> Self {
        match err {
            NonceError::InvalidAddress(_) => TransactionError::InvalidAddress,
            NonceError::HttpRequestError(_) => TransactionError::NetworkIssue,
            NonceError::InvalidResponse => TransactionError::NetworkIssue,
        }
    }
}

impl std::error::Error for TransactionError {}
impl std::error::Error for DynamoDbError {}

#[derive(Debug, Serialize, Deserialize)]
pub enum WalletError {
    MissingToken,
    InvalidToken(String),
    InvalidWalletAddress,
    WalletAlreadyExists,
    CognitoUpdateFailed(String),
    MissingWallet(String),
    Network(String),
    InvalidResponse(String),
    IncompleteResponse(String)
}

impl fmt::Display for WalletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalletError::InvalidWalletAddress => write!(f, "Invalid wallet address format."),
            WalletError::WalletAlreadyExists => write!(f, "A wallet already exists for this user."),
            WalletError::CognitoUpdateFailed(msg) => write!(f, "Failed to update Cognito: {}", msg),
            WalletError::MissingToken => write!(f, "Authorization token is missing."),
            WalletError::InvalidToken(_) => write!(f, "Authorization token is invalid."),
            WalletError::MissingWallet(msg) => write!(f, "Wallet not found: {}", msg),
            WalletError::Network(msg) =>  write!(f, "Network error: {}", msg),
            WalletError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            WalletError::IncompleteResponse(msg) => write!(f, "Incomplete: {}", msg),
        }
    }
}

impl From<AuthorizationError> for WalletError {
    fn from(err: AuthorizationError) -> Self {
        match err {
            AuthorizationError::Unauthorized(msg) => WalletError::InvalidToken(msg),
        }
    }
}

impl std::error::Error for WalletError {}


#[derive(Serialize, Deserialize)]
pub enum ValidateError {
    MissingIdToken,
    TokenValidationFailed(String),
    TokenDecodingFailed(String),
    CognitoCheckFailed(String),
    TokenGenerationFailed(String),
}

impl fmt::Display for ValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidateError::MissingIdToken => write!(f, "Missing id_token"),
            ValidateError::TokenValidationFailed(e) => write!(f, "Token validation failed: {}", e),
            ValidateError::TokenDecodingFailed(e) => write!(f, "Token decoding failed: {}", e),
            ValidateError::CognitoCheckFailed(e) => write!(f, "Cognito check failed: {}", e),
            ValidateError::TokenGenerationFailed(e) => write!(f, "Token generation failed: {}", e),
        }
    }
}

impl Debug for ValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidateError::CognitoCheckFailed(e) => write!(f, "CognitoCheckFailed: {:?}", e),
            ValidateError::MissingIdToken => write!(f, "MissingIdToken"),
            ValidateError::TokenValidationFailed(e) => write!(f, "TokenValidationFailed: {:?}", e),
            ValidateError::TokenDecodingFailed(e) => write!(f, "TokenDecodingFailed: {:?}", e),
            ValidateError::TokenGenerationFailed(e) => write!(f, "TokenGenerationFailed: {:?}", e),
        }
    }
}

impl std::error::Error for ValidateError {}

#[derive(Debug, Serialize, Deserialize)]
pub enum PhoneNumberError {
    InvalidPhoneNumber(String),
    InvalidCountryCode,
    InvalidNumberLength,
    CognitoUpdateFailed(String),
    DynamoDBUpdateFailed(String),
    DynamoDBReadFailed(String),
    ParseError(String),
}

impl From<ParseError> for PhoneNumberError {
    fn from(err: ParseError) -> Self {
        PhoneNumberError::InvalidPhoneNumber(format!("Invalid phone number {:?}", err))
    }
}

impl From<AuthorizationError> for PhoneNumberError {
    fn from(err: AuthorizationError) -> Self {
        match err {
            AuthorizationError::Unauthorized(msg) => PhoneNumberError::CognitoUpdateFailed(msg),
        }
    }
}

impl fmt::Display for PhoneNumberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhoneNumberError::CognitoUpdateFailed(msg) => write!(f, "Cognito update failed: {}", msg),
            PhoneNumberError::InvalidCountryCode => write!(f, "Invalid country code"),
            PhoneNumberError::InvalidNumberLength => write!(f, "Invalid phone number length"),
            PhoneNumberError::InvalidPhoneNumber(msg) => write!(f, "Invalid phone number: {}", msg),
            PhoneNumberError::DynamoDBUpdateFailed(msg) => write!(f, "DynamoDB update failed: {}", msg),
            PhoneNumberError::DynamoDBReadFailed(msg) => write!(f, "DynamoDB read failed: {}", msg),
            PhoneNumberError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}
impl std::error::Error for PhoneNumberError {}


#[derive(Debug, Error)]
pub enum FetchRateError {
    #[error("Request failed: {0}")]
    RequestError(#[from] ReqwestError),  // Automatically converts reqwest::Error

    #[error("Invalid response: {0}")]
    IoError(#[from] io::Error), // For other errors

    #[error("Missing exchange rate data")]
    MissingRate,
}


#[derive(Debug, Error)]
pub enum GasEstimateError {
    #[error("HTTP request failed: {0}")]
    HttpRequestFailed(#[from] reqwest::Error),

    #[error("Failed to parse gas API response: {0}")]
    ParseError(String, String),

    #[error("Gas API returned an invalid response")]
    InvalidResponse(String, String),

    #[error("Gas API returned an incomplete response")]
    IncompleteResponse(String),

    #[error("API has raised an error")]
    ApiError(String, String),

    #[error("Internal API Error")]
    RequestError(String, String),

    #[error("Network error")]
    Network(String),
}

impl From<GasEstimateError> for TransactionError {
    fn from(err: GasEstimateError) -> Self {
        TransactionError::GasPriceUnavailable(err.to_string())
    }
}


#[derive(Debug, Error)]
pub enum NonceError {
    #[error("Failed to parse address: {0}")]
    InvalidAddress(String),

    #[error("HTTP request failed: {0}")]
    HttpRequestError(String),

    #[error("Unexpected RPC response format")]
    InvalidResponse,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("AWS DynamoDB error: {0}")]
    DynamoDb(String),

    #[error("Ethereum provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Reqwest error: {0}")]
    Http(#[from] ReqwestError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] SerdeJsonError),

    #[error("Missing environment variable: {0}")]
    MissingEnv(String),

    #[error("Parse error: {0}")]
    ParseError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("Transaction logic error: {0}")]
    Logic(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<std::env::VarError> for AppError {
    fn from(err: std::env::VarError) -> Self {
        AppError::MissingEnv(err.to_string())
    }
}

impl<T> From<DynamoError<T>> for AppError
where
    T: std::error::Error + Send + Sync + 'static,
{
    fn from(err: DynamoError<T>) -> Self {
        AppError::DynamoDb(err.to_string())
    }
}
