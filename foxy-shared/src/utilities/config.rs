
use dotenv::dotenv;
use std::env;
use crate::models::transactions::{Network, TokenType};

/// Initialize dotenv (only needs to be called once at startup)
pub fn init() {
    if dotenv().is_ok() {
        println!("Loaded .env file");
    } else {
        println!("Failed to load .env file");
    }
}

/// Fetch environment variables by key
pub fn get_env_var(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("Environment variable {} must be set", key))
}
//Get table names
pub fn get_transaction_event_table() -> String {
    get_env_var("EVENT_STORE_TABLE_NAME")
}

pub fn get_user_lookup_table() -> String {
    get_env_var("DYNAMODB_USER_LOOKUP_TABLE_NAME")
}
/// Get Google Client ID
pub fn get_google_client_id() -> String {
    get_env_var("GOOGLE_CLIENT_ID")
}
pub fn get_user_pool_id() -> String {
    get_env_var("COGNITO_USER_POOL_ID")
}

pub fn get_user_pool_client_id() -> String {
    get_env_var("COGNITO_USER_POOL_CLIENT_ID")
}

pub fn get_aws_region() -> String {
    get_env_var("AWS_REGION")
}

pub fn get_rpc_url() -> String {
    let network = env::var("NETWORK").unwrap_or_else(|_| "mainnet".to_string());

    match network.as_str() {
        "mainnet" => get_env_var("INFURA_RPC_MAINNET"),
        "testnet" => get_env_var("INFURA_RPC_TESTNET"),
        _ => panic!("Invalid NETWORK value: must be 'mainnet' or 'testnet'"),
    }
}

pub fn get_test_rpc_url() -> String {
    //when you know you want the test network
    get_env_var("INFURA_RPC_TESTNET")
}

pub fn get_ethereum_url() -> String {
    env::var("ETHEREUM_RPC_MAINNET").unwrap_or_else(|_| "https://mainnet.infura.io/v3/60751eb31a574890b941ec68e4f5dc18".to_string())
}

pub fn get_broadcast_queue() -> String {
    get_env_var("BROADCAST_QUEUE_URL")
}

pub fn get_visibility_timeout() -> String {
    //when you know you want the test network
    get_env_var("VISIBILITY_TIMEOUT_SECS")
}

pub fn get_chain_id() -> u64 {
    let network = env::var("NETWORK").unwrap_or_else(|_| "mainnet".to_string());

    match network.as_str() {
        "mainnet" => get_env_var("OPTIMISM_CHAIN_MAINNET").parse().unwrap_or(10),
        "testnet" => get_env_var("OPTIMISM_CHAIN_TESTNET").parse().unwrap_or(420),
        _ => panic!("Invalid NETWORK value"),
    }
}

pub fn get_network() -> Network {
    let network = env::var("NETWORK").unwrap_or_else(|_| "mainnet".to_string());

    match network.as_str() {
        "mainnet" => Network::OptimismMainnet,
        "testnet" => Network::OptimismSepolia,
        _ => panic!("Invalid NETWORK value"),
    }
}

pub fn get_default_token() -> TokenType {
    TokenType::ETH
}