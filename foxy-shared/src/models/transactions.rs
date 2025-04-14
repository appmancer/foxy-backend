use crate::utilities::parsers::u128_from_str;
use crate::models::estimate_flags::serialize_flags_as_strings;
use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;
use crate::utilities::id_generator::generate_transaction_id;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::models::errors::TransactionError;
use crate::models::estimate_flags::EstimateFlags;
use crate::services::cognito_services::get_party_details_from_wallet;
use crate::utilities::config::{get_chain_id, get_network};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use ethers_core::types::H256;
use ethers_core::utils::keccak256;

/// Comprehensive transaction statuses, including Layer 2 (Optimism) specifics
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Created,              // Transaction intent created but not signed
    Signed,               // Transaction signed by the user's wallet
    Pending,              // Sent to the network, awaiting confirmation (in mempool)
    Confirmed,            // Mined with 1 confirmation (considered settled)
    Finalized,            // Multiple confirmations; immutable

    Failed,               // Failed due to execution error (out of gas, etc.)
    Cancelled,            // Replaced or canceled by user/system
    Error,                // System-level error (network, timeout)
    FailedToCollectFee,    // Unable to complete fee tx

    // Layer 2 (Optimism) Specific States
    Deposited,            // Bridged from L1 to L2, pending settlement
    Finalizing,           // In fraud-proof window (Optimistic rollup)
    Withdrawn,            // Withdrawn to L1, pending settlement
    ChallengePeriod,      // In challenge/validation phase
    Bridging              // Ongoing cross-chain transfer (L1/L2 or L2/L2)
}
impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionStatus::Created => write!(f, "Created"),
            TransactionStatus::Signed => write!(f, "Signed"),
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Confirmed => write!(f, "Confirmed"),
            TransactionStatus::Finalized => write!(f, "Finalized"),
            TransactionStatus::Failed => write!(f, "Failed"),
            TransactionStatus::Cancelled => write!(f, "Cancelled"),
            TransactionStatus::Error => write!(f, "Error"),
            TransactionStatus::FailedToCollectFee => write!(f, "FailedToCollectFee"),
            TransactionStatus::Deposited => write!(f, "Deposited"),
            TransactionStatus::Finalizing => write!(f, "Finalizing"),
            TransactionStatus::Withdrawn => write!(f, "Withdrawn"),
            TransactionStatus::ChallengePeriod => write!(f, "ChallengePeriod"),
            TransactionStatus::Bridging => write!(f, "Bridging"),
        }
    }
}

impl FromStr for TransactionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "created" => Ok(TransactionStatus::Created),
            "signed" => Ok(TransactionStatus::Signed),
            "pending" => Ok(TransactionStatus::Pending),
            "confirmed" => Ok(TransactionStatus::Confirmed),
            "finalized" => Ok(TransactionStatus::Finalized),
            "failed" => Ok(TransactionStatus::Failed),
            "cancelled" => Ok(TransactionStatus::Cancelled),
            "error" => Ok(TransactionStatus::Error),
            "failed-to-collect-fee" => Ok(TransactionStatus::FailedToCollectFee),
            "deposited" => Ok(TransactionStatus::Deposited),
            "finalizing" => Ok(TransactionStatus::Finalizing),
            "withdrawn" => Ok(TransactionStatus::Withdrawn),
            "challenge_period" => Ok(TransactionStatus::ChallengePeriod),
            "bridging" => Ok(TransactionStatus::Bridging),
            other => Err(format!("Unknown transaction status: {}", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum EventType {
    Creation,
    Signing,
    Broadcasting,
    Confirmation,
    Finalization,
    Failure,
    Cancellation,
    Erroring,
    Deposit,
    Finalizing,
    Withdrawal,
    Challenge,
    Bridging,
}


impl FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "creation" => Ok(EventType::Creation),
            "signing" => Ok(EventType::Signing),
            "broadcasting" => Ok(EventType::Broadcasting),
            "confirmation" => Ok(EventType::Confirmation),
            "finalization" => Ok(EventType::Finalization),
            "failure" => Ok(EventType::Failure),
            "cancellation" => Ok(EventType::Cancellation),
            "erroring" => Ok(EventType::Erroring),
            "deposit" => Ok(EventType::Deposit),
            "finalizing" => Ok(EventType::Finalizing),
            "withdrawal" => Ok(EventType::Withdrawal),
            "challenge" => Ok(EventType::Challenge),
            "bridging" => Ok(EventType::Bridging),
            _ => Err(format!("Invalid token type: {}", s)),
        }
    }
}


impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::Creation => write!(f, "Creation"),
            EventType::Signing => write!(f, "Signing"),
            EventType::Broadcasting => write!(f, "Broadcasting"),
            EventType::Confirmation => write!(f, "Confirmation"),
            EventType::Finalization => write!(f, "Finalization"),
            EventType::Failure => write!(f, "Failure"),
            EventType::Cancellation => write!(f, "Cancellation"),
            EventType::Erroring => write!(f, "Erroring"),
            EventType::Deposit => write!(f, "Deposit"),
            EventType::Finalizing => write!(f, "Finalizing"),
            EventType::Withdrawal => write!(f, "Withdrawal"),
            EventType::Challenge => write!(f, "Challenge"),
            EventType::Bridging => write!(f, "Bridging"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum TokenType {
    #[default]
    ETH,
    USDC,
}

impl TokenType {
    pub fn decimals(&self) -> u8 {
        match self {
            TokenType::ETH => 18,
            TokenType::USDC => 6,
        }
    }
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::ETH => write!(f, "ETH"),
            TokenType::USDC => write!(f, "USDC"),
        }
    }
}

impl FromStr for TokenType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ETH" => Ok(TokenType::ETH),
            "USDC" => Ok(TokenType::USDC),
            _ => Err(format!("Invalid token type: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub transaction_id: String,
    pub user_id: String,
    pub sender_address: String,
    pub recipient_address: String,
    pub transaction_value: u128, // Amount in the selected token base units (e.g wei)
    pub token_type: TokenType, // ETH or USDC
    pub status: TransactionStatus,
    pub network_fee: u128, // Gas fees in base units
    pub service_fee: u128, // Additional platform fees
    pub total_fees: u128, // Sum of network + service fees
    pub fiat_value: u64, // Equivalent fiat value
    pub fiat_currency: String, // E.g., GBP, USD
    pub chain_id: u64, // Supports multi-chain transactions
    pub signed_tx: Option<String>, //the signed data
    pub transaction_hash: Option<String>, // Assigned after broadcast
    pub event_log: Option<String>, // Stores blockchain receipt events
    pub metadata: Metadata, // Additional transaction metadata
    pub priority_level: PriorityLevel, // Transaction priority (e.g., 1-5 scale)
    pub network: Network, // Network name (Optimism, Ethereum, etc.)
    pub gas_price: Option<u64>, // Gas price used for the transaction
    pub gas_used: Option<u64>, // Gas consumed by the transaction
    pub gas_limit: Option<u64>, // Gas limit set for the transaction
    pub nonce: Option<u64>, // Nonce used for ordering transactions
    pub max_fee_per_gas: Option<u64>, // EIP-1559: Max fee willing to pay per gas unit
    pub max_priority_fee_per_gas: Option<u64>, // EIP-1559: Priority fee for miners
    pub total_fee_paid: Option<u64>, // total fees for simple view
    pub exchange_rate: Option<f64>, // rate at time of tx
    pub block_number: Option<u64>, // Block number the transaction was included in
    pub receipt_status: Option<u8>, // Status from the transaction receipt (1 = success, 0 = fail)
    pub contract_address: Option<String>, // Required for ERC-20 transactions (e.g., USDC contract)
    pub approval_tx_hash: Option<String>, // Transaction hash for ERC-20 approval (if needed)
    pub recipient_tx_hash: Option<String>,
    pub fee_tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}


impl Default for Transaction {
    fn default() -> Self {
        Self {
            transaction_id: "tx_default".into(),
            user_id: "user_default".into(),
            sender_address: "0x0000000000000000000000000000000000000000".into(),
            recipient_address: "0x0000000000000000000000000000000000000000".into(),
            transaction_value: 0,
            token_type: TokenType::ETH,
            status: TransactionStatus::Created,
            network_fee: 0,
            service_fee: 0,
            total_fees: 0,
            fiat_value: 0,
            fiat_currency: "GBP".into(),
            chain_id: get_chain_id(),
            signed_tx: None,
            transaction_hash: None,
            event_log: None,
            metadata: Metadata::default(),
            priority_level: PriorityLevel::Standard,
            network: Network::OptimismSepolia,
            gas_price: None,
            gas_used: None,
            gas_limit: None,
            nonce: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            total_fee_paid: None,
            exchange_rate: None,
            block_number: None,
            receipt_status: None,
            contract_address: None,
            approval_tx_hash: None,
            recipient_tx_hash: None,
            fee_tx_hash: None,
            created_at: Utc::now(),
        }
    }
}

impl TransactionBuilder {
    pub async fn build(self, cognito_client: &CognitoClient, dynamo_db_client: &DynamoDbClient) -> Result<Transaction, TransactionError> {
        let from = get_party_details_from_wallet(&cognito_client, &dynamo_db_client, &self.sender_address).await?;
        let to = get_party_details_from_wallet(&cognito_client, &dynamo_db_client, &self.recipient_address).await?;

        Ok(Transaction {
            transaction_id: generate_transaction_id().to_string(),
            user_id: self.sender_user_id.clone(),
            sender_address: self.sender_address,
            recipient_address: self.recipient_address,
            transaction_value: self.transaction_value,
            token_type: self.token_type,
            status: TransactionStatus::Created,
            network_fee: self.network_fee,
            service_fee: self.service_fee,
            total_fees: self.service_fee + self.network_fee,
            fiat_value:self.fiat_value,
            fiat_currency: self.fiat_currency_code.clone(),
            chain_id: get_chain_id(),
            signed_tx: None,
            transaction_hash: None,
            event_log: None,
            metadata: Metadata{
                message: self.message.unwrap_or_default(),
                display_currency: self.fiat_currency_code.clone(),
                expected_currency_amount: self.fiat_value,
                from,
                to,
            },
            priority_level: self.priority_level,
            network: self.network,
            gas_price: Some(self.gas_estimate.gas_price),
            gas_used: None,
            gas_limit: Some(self.gas_estimate.gas_limit),
            nonce: Some(self.nonce),
            max_fee_per_gas: Some(self.gas_estimate.max_fee_per_gas),
            max_priority_fee_per_gas: Some(self.gas_estimate.max_priority_fee_per_gas),
            total_fee_paid: None,
            exchange_rate: Some(self.rate),
            block_number: None,
            receipt_status: None,
            contract_address: None,
            approval_tx_hash: None,
            recipient_tx_hash: None,
            fee_tx_hash: None,
            created_at: Utc::now(),
        })
    }
}

impl Transaction {
    pub fn with_status(mut self, status: TransactionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_signed_tx(mut self, signed: &str) -> Self {
        self.signed_tx = Some(signed.parse().unwrap());
        self
    }

    pub fn with_transaction_hash(mut self, signed: &str) -> Self {
        self.transaction_hash = Some(signed.parse().unwrap());
        self
    }

    pub fn tx_hash(&self) -> Option<H256> {
        let signed_tx = self.signed_tx.as_ref()?;
        let raw = hex::decode(signed_tx.trim_start_matches("0x")).ok()?;
        Some(H256::from(keccak256(&raw)))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransactionRequest {
    pub sender_address: String,
    pub recipient_address: String,
    pub fiat_value: u64, //in minor units (e.g. cents or pence)
    pub fiat_currency_code: String,
    #[serde(deserialize_with = "u128_from_str")]
    pub transaction_value: u128,
    pub token_type: TokenType,
    pub message: Option<String>,

    // The user-visible quote data, stringified for mobile compatibility
    pub gas_pricing: Option<GasPricing>,

    // The backend-validated gas data, used for fee math and tx building
    pub gas_estimate: Option<GasEstimate>,

    pub exchange_rate: f64,
    #[serde(deserialize_with = "u128_from_str")]
    pub service_fee: u128,
    #[serde(deserialize_with = "u128_from_str")]
    pub network_fee: u128
}

pub struct TransactionBuilder {
    pub sender_address: String,
    pub sender_user_id: String,
    pub recipient_address: String,
    pub fiat_value: u64,
    pub fiat_currency_code: String,
    pub transaction_value: u128,
    pub message: Option<String>,
    pub rate: f64,
    pub network: Network,
    pub token_type: TokenType,
    pub priority_level: PriorityLevel,
    pub nonce: u64,
    pub gas_estimate: GasEstimate,
    pub exchange_rate: f64,
    pub service_fee: u128,
    pub network_fee: u128,
}

impl TryFrom<TransactionRequest> for TransactionBuilder {
    type Error = TransactionError;

    fn try_from(req: TransactionRequest) -> Result<Self, Self::Error> {
        // Try to use gas_estimate, otherwise fallback to gas_pricing
        let gas_estimate = match req.gas_estimate {
            Some(est) => est,
            None => {
                if let Some(pricing) = req.gas_pricing {
                    GasEstimate::try_from(pricing).map_err(|err| {
                        log::error!("GasEstimate conversion failed: {:?}", err);
                        TransactionError::MissingGasEstimate
                    })?
                } else {
                    log::error!("Missing both gas_estimate and gas_pricing in request");
                    return Err(TransactionError::MissingGasEstimate);
                }
            }
        };

        Ok(Self {
            sender_address: req.sender_address,
            sender_user_id: String::default(), // to be filled after auth
            recipient_address: req.recipient_address,
            fiat_value: req.fiat_value,
            fiat_currency_code: req.fiat_currency_code,
            transaction_value: req.transaction_value,
            message: req.message,
            rate: req.exchange_rate,
            network: get_network(),
            token_type: req.token_type,
            priority_level: PriorityLevel::Standard,
            nonce: 0,
            gas_estimate,
            service_fee: req.service_fee,
            network_fee: req.network_fee,
            exchange_rate: req.exchange_rate,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Network {
    EthereumMainnet,
    EthereumSepolia,
    OptimismMainnet,
    OptimismSepolia,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PriorityLevel {
    Standard,  // default, safe gas fee
    Fast,      // more responsive, higher gas
    Urgent     // top speed, cost is no issue
}

/// Unsigned transaction details
/// Value is in string format to retain blockchain compatibility

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnsignedTransaction {
    pub transaction_id: String,
    pub tx_type: u8, // EIP-1559 = 2
    pub to: String,
    pub amount_base_units: String,
    pub gas_limit: String,
    pub gas_price: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
    pub nonce: String,
    pub chain_id: String,
    pub token_type: TokenType,
    pub token_decimals: u8,
}

impl From<&Transaction> for UnsignedTransaction {
    fn from(tx: &Transaction) -> Self {
        UnsignedTransaction {
            transaction_id: tx.transaction_id.clone(),
            tx_type: 2,
            to: tx.recipient_address.clone(),
            amount_base_units: tx.transaction_value.to_string(),
            gas_limit: tx.gas_limit.unwrap_or(0).to_string(),
            gas_price: tx.gas_price.unwrap_or(0).to_string(),
            max_fee_per_gas: tx.max_fee_per_gas.unwrap_or(0).to_string(),
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas.unwrap_or(0).to_string(),
            nonce: tx.nonce.unwrap_or_default().to_string(),
            chain_id: tx.chain_id.to_string(),
            token_type: tx.token_type.clone(),
            token_decimals: match tx.token_type {
                TokenType::ETH => 18,
                TokenType::USDC => 6,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionEstimateRequest {
    pub fiat_value: u64, //in minor units (e.g. cents or pence)
    pub fiat_currency: String,
    pub sender_address: String,
    pub recipient_address: String,
    pub token_type: TokenType,

    #[serde(skip_serializing_if = "Option::is_none")] // Skips field if None
    pub transaction_value: Option<u128>, // Calculated from fiat_amount and exchange rate
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FeeBreakdown {
    pub service_fee_wei: String,
    pub service_fee_eth: String,
    pub network_fee_wei: String,
    pub network_fee_eth: String,
    pub total_fee_wei: String,
    pub total_fee_eth: String,
}

//A note to myself, as I forget why this exists.  The Android client doesn't cope well with the
//large number formats.  This is memento class so that the client app has something to display.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct GasPricing {
    pub estimated_gas: String,
    pub gas_price: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TransactionEstimateResponse {
    pub token_type: TokenType,
    pub fiat_amount_minor: u64,                // e.g. 500 = £5.00
    pub fiat_currency: String,                 // "GBP"
    pub eth_amount: String,                    // "0.00344"
    pub wei_amount: String,                    // "3440000000000000"

    pub fees: FeeBreakdown,
    pub gas: GasPricing,

    pub exchange_rate: f64,                 // 1453.23
    pub exchange_rate_expires_at: DateTime<Utc>,

    pub recipient_address: String,
    #[serde(serialize_with = "serialize_flags_as_strings")]
    pub status: EstimateFlags,
    pub message: Option<String>,
}

/// Transaction metadata fully aligned with the example data
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metadata {
    pub message: String,
    pub display_currency: String,
    pub expected_currency_amount: u64, //in minor units (e.g. cents or pence)
    pub from: PartyDetails,
    pub to: PartyDetails,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            message: "".to_string(),
            display_currency: "GBP".to_string(),
            expected_currency_amount: 0,
            from: PartyDetails::default(),
            to: PartyDetails::default(),
        }
    }
}

/// Detailed information about sender and recipient
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartyDetails {
    pub name: String,
    pub wallet: String,
}

impl Default for PartyDetails {
    fn default() -> Self {
        Self {
            name: "Anonymous".to_string(),
            wallet: "0x0000000000000000000000000000000000000000".to_string(),
        }
    }
}

/// Geo-location
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GeoLocation {
    pub latitude: String,
    pub longitude: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GasEstimate {
    pub status: EstimateFlags,
    pub gas_limit: u64,  // Fixed gas limit for ETH transfers
    pub gas_price: u64,  // L2 base fee per gas (in WEI)
    #[serde(deserialize_with = "u128_from_str")]
    pub l1_fee: u128,     // L1 data fee (in WEI)
    pub max_fee_per_gas: u64,  // Usually same as gas_price
    pub max_priority_fee_per_gas: u64,  // 0 on Optimism
    #[serde(deserialize_with = "u128_from_str")]
    pub network_fee: u128,  // Total transaction fee (L2 + L1)
}

impl TryFrom<GasPricing> for GasEstimate {
    type Error = anyhow::Error;

    fn try_from(pricing: GasPricing) -> Result<Self, Self::Error> {
        let gas_limit = pricing.estimated_gas.parse::<u64>()?;
        let gas_price = pricing.gas_price.parse::<u64>()?;
        let max_fee_per_gas = pricing.max_fee_per_gas.parse::<u64>()?;
        let max_priority_fee_per_gas = pricing.max_priority_fee_per_gas.parse::<u64>()?;

        let l1_fee: u128 = 0;
        let network_fee: u128 = (gas_limit as u128) * (gas_price as u128);

        Ok(GasEstimate {
            status: EstimateFlags::SUCCESS,
            gas_limit,
            gas_price,
            l1_fee,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            network_fee,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_transaction_request() {
        let raw = json!({
            "sender_address": "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0",
            "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
            "fiat_value": 5000,
            "fiat_currency_code": "GBP",
            "transaction_value": "1000000000000000",
            "token_type": "ETH",
            "message": "Let's get coffee",
            "exchange_rate": 2300.0,
            "network_fee": "21000000000000",
            "service_fee": "10000000000000",
            "gas_estimate": {
                "gas_limit": 21000,
                "gas_price": 1000000000,
                "max_fee_per_gas": 1000000000,
                "max_priority_fee_per_gas": 1000000000,
                "l1_fee": "0",
                "network_fee": "21000000000000",
                "status": "SUCCESS"
            }
        });

        let parsed: Result<TransactionRequest, _> = serde_json::from_value(raw);

        match parsed {
            Ok(req) => {
                println!("Parsed TransactionRequest: {:#?}", req);
                assert_eq!(req.fiat_currency_code, "GBP");
                assert_eq!(req.token_type, TokenType::ETH);
            }
            Err(e) => {
                panic!("Deserialization failed: {}", e);
            }
        }
    }

    #[test]
    fn test_tx_hash_derives_correctly() {
        let raw = json!({
            "transaction_id": "3e38a355-f26f-4ac2-ac55-1edb5bcfd09f",
            "user_id": "112527246877271240195",
            "sender_address": "0xe006487c4CEC454574b6C9A9F79fF8A5DEe636A0",
            "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
            "transaction_value": 1000,
            "token_type": "ETH",
            "status": "Broadcasted",
            "network_fee": 21000,
            "service_fee": 0,
            "total_fees": 21000,
            "fiat_value": 1,
            "fiat_currency": "GBP",
            "chain_id": 11155420,
            "signed_tx": "0xf86b0f830f424082520894a826d3484625b29dfcbdaee6ca636a1acb439bf885e8d4a51000808401546fdca0f11a428a380a093705b21b1d59ad21240ec5fb6a88230b6e97616ff0384c4618a02b44589337b649c9e5cdb9e0c9e191c3ccf9e2676aed5c6e4b6f3c58368fd69a",
            "transaction_hash": "0x797f4cc25a85a46c2812cc5d2668fc82a93351368557a4e1aead0fed7c64505d",
            "event_log": null,
            "metadata": {
                "message": "",
                "display_currency": "GBP",
                "expected_currency_amount": 0,
                "from": {
                    "name": "Anonymous",
                    "wallet": "0x0000000000000000000000000000000000000000"
                },
                "to": {
                    "name": "Anonymous",
                    "wallet": "0x0000000000000000000000000000000000000000"
                }
            },
            "priority_level": "Standard",
            "network": "OptimismSepolia",
            "gas_price": 1000000,
            "gas_used": null,
            "gas_limit": 21000,
            "nonce": null,
            "max_fee_per_gas": 1000000,
            "max_priority_fee_per_gas": 150000,
            "total_fee_paid": null,
            "exchange_rate": null,
            "block_number": null,
            "receipt_status": null,
            "contract_address": null,
            "approval_tx_hash": null,
            "recipient_tx_hash": null,
            "fee_tx_hash": null,
            "created_at": "2025-04-11T16:29:35.096687121Z"
        });
        let tx: Transaction = serde_json::from_value(raw).unwrap();
        let hash = tx.tx_hash().unwrap();
        assert_eq!(format!("{:#x}", hash), "0x797f4cc25a85a46c2812cc5d2668fc82a93351368557a4e1aead0fed7c64505d"); // expected value
    }

}
