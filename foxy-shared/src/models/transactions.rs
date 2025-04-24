use crate::utilities::parsers::u128_from_str;
use crate::models::estimate_flags::serialize_flags_as_strings;
use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::models::errors::TransactionError;
use crate::models::estimate_flags::EstimateFlags;
use crate::services::cognito_services::get_party_details_from_wallet;
use crate::utilities::config::{get_chain_id, get_foxy_wallet, get_network};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use ethers_core::types::H256;
use ethers_core::utils::keccak256;
use log::warn;
use uuid::Uuid;
use crate::database::transaction_event::TransactionEventManager;
use crate::utilities::nonce_manager::NonceManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBundle {
    pub bundle_id: String,
    pub user_id: String,
    pub status: BundleStatus,
    pub fee_tx: Transaction,
    pub main_tx: Transaction,
    pub metadata: Option<BundleMetadata>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TransactionBundle {
    pub fn new(
        user_id: String,
        fee_tx: Transaction,
        main_tx: Transaction,
        metadata: Option<BundleMetadata>,
    ) -> Self {
        Self {
            bundle_id: Uuid::new_v4().to_string(),
            user_id,
            status: BundleStatus::Initiated,
            fee_tx,
            main_tx,
            metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub async fn from_request(
        user_id: String,
        request: TransactionRequest,
        cognito_client: &CognitoClient,
        dynamo_db_client: &DynamoDbClient,
    ) -> Result<Self, TransactionError> {
        let sender_details = get_party_details_from_wallet(
            cognito_client,
            dynamo_db_client,
            &request.sender_address,
        )
            .await?;

        let recipient_details = get_party_details_from_wallet(
            cognito_client,
            dynamo_db_client,
            &request.recipient_address,
        )
            .await?;

        let gas_pricing = request
            .gas_pricing
            .as_ref()
            .ok_or_else(|| TransactionError::MissingGasEstimate)?;
        let fee_tx_value = request.service_fee + request.network_fee;

        let nonces = NonceManager::new()?;
        let nonce = nonces.get_nonce(&request.sender_address).await?;

        let fee_tx = Transaction::new(
            user_id.clone(),
            sender_details.wallet.clone(),
            get_foxy_wallet(),
            fee_tx_value,
            request.token_type.clone(),
            request.fiat_value,
            request.fiat_currency_code.clone(),
            nonce + 1, //perform the main transaction first
        ).with_gas_pricing(gas_pricing);

        let main_tx = Transaction::new(
            user_id.clone(),
            get_foxy_wallet(),
            recipient_details.wallet.clone(),
            request.transaction_value,
            request.token_type.clone(),
            request.fiat_value,
            request.fiat_currency_code.clone(),
            nonce,
        ).with_gas_pricing(gas_pricing);

        let metadata = BundleMetadata {
            display_currency: request.fiat_currency_code,
            expected_currency_amount: request.fiat_value,
            message: request.message,
            sender: Some(sender_details),
            recipient: Some(recipient_details),
            app_version: None,
            location: None,
            service_fee: request.service_fee,
            network_fee: request.network_fee,
            exchange_rate: request.exchange_rate,
            gas_pricing: gas_pricing.clone(),
        };

        Ok(TransactionBundle {
            bundle_id: Uuid::new_v4().to_string(),
            user_id,
            status: BundleStatus::Initiated,
            fee_tx,
            main_tx,
            metadata: Some(metadata),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub display_currency: String,
    pub expected_currency_amount: u64,
    pub message: Option<String>,
    pub sender: Option<PartyDetails>,
    pub recipient: Option<PartyDetails>,
    pub app_version: Option<String>,
    pub location: Option<GeoLocation>,
    pub service_fee: u128,
    pub network_fee: u128,
    pub exchange_rate: f64,
    pub gas_pricing: GasPricing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BundleStatus {
    Initiated,
    Signed,
    MainConfirmed,
    Completed,
    Failed,
    Cancelled,
    Errored
}

impl fmt::Display for BundleStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            BundleStatus::Initiated => "Initiated",
            BundleStatus::Signed => "Signed",
            BundleStatus::MainConfirmed => "MainConfirmed",
            BundleStatus::Completed => "Completed",
            BundleStatus::Failed => "Failed",
            BundleStatus::Cancelled => "Cancelled",
            BundleStatus::Errored => "Errored",
        };
        write!(f, "{}", s)
    }
}


impl FromStr for BundleStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "initiated" => Ok(BundleStatus::Initiated),
            "signed" => Ok(BundleStatus::Signed),
            "mainconfirmed" => Ok(BundleStatus::MainConfirmed),
            "completed" => Ok(BundleStatus::Completed),
            "failed" => Ok(BundleStatus::Failed),
            "cancelled" => Ok(BundleStatus::Cancelled),
            "errored" => Ok(BundleStatus::Errored),
            _ => Err(format!("Invalid bundle status: {}", s)),
        }
    }
}

/// Comprehensive transaction statuses, including Layer 2 (Optimism) specifics
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Created,     // Intent created, unsigned
    Signed,      // Client signed the tx
    Pending,     // Sent to the network, awaiting confirmation
    Confirmed,   // Mined with 1 confirmation
    Failed,      // Reverted or out of gas
    Cancelled,   // User/system abort
    Error        // Infra/systemic issue
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionLeg {
    Fee,
    Main,
}

impl fmt::Display for TransactionLeg {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            TransactionLeg::Fee => "Fee",
            TransactionLeg::Main => "Main",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for TransactionLeg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fee" => Ok(TransactionLeg::Fee),
            "main" => Ok(TransactionLeg::Main),
            _ => Err(format!("Invalid transaction leg: {}", s)),
        }
    }
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TransactionStatus::Created => write!(f, "Created"),
            TransactionStatus::Signed => write!(f, "Signed"),
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Confirmed => write!(f, "Confirmed"),
            TransactionStatus::Failed => write!(f, "Failed"),
            TransactionStatus::Cancelled => write!(f, "Cancelled"),
            TransactionStatus::Error => write!(f, "Error"),
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
            "failed" => Ok(TransactionStatus::Failed),
            "cancelled" => Ok(TransactionStatus::Cancelled),
            "error" => Ok(TransactionStatus::Error),
            other => Err(format!("Unknown transaction status: {}", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum EventType {
    Initiate,
    Sign,
    Broadcast,
    Confirm,
    Fail,
    Cancel,
    Error
}

impl FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "initiate" => Ok(EventType::Initiate),
            "sign" => Ok(EventType::Sign),
            "broadcast" => Ok(EventType::Broadcast),
            "confirm" => Ok(EventType::Confirm),
            "fail" => Ok(EventType::Fail),
            "cancel" => Ok(EventType::Cancel),
            "error" => Ok(EventType::Error),
            _ => Err(format!("Invalid event type: {}", s)),
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EventType::Initiate => write!(f, "Initiate"),
            EventType::Sign => write!(f, "Sign"),
            EventType::Broadcast => write!(f, "Broadcast"),
            EventType::Confirm => write!(f, "Confirm"),
            EventType::Fail => write!(f, "Fail"),
            EventType::Cancel => write!(f, "Cancel"),
            EventType::Error => write!(f, "Error"),
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

impl Transaction {
    pub fn new(
        user_id: String,
        sender_address: String,
        recipient_address: String,
        transaction_value: u128,
        token_type: TokenType,
        fiat_value: u64,
        fiat_currency: String,
        nonce: u64,
    ) -> Self {
        Self {
            transaction_id: Uuid::new_v4().to_string(),
            user_id,
            sender_address,
            recipient_address,
            transaction_value,
            token_type,
            status: TransactionStatus::Created,

            network_fee: 0,
            service_fee: 0,
            total_fees: 0,
            fiat_value,
            fiat_currency,

            chain_id: get_chain_id(),
            signed_tx: None,
            transaction_hash: None,
            event_log: None,

            priority_level: PriorityLevel::Standard,
            network: get_network(),

            gas_price: None,
            gas_used: None,
            gas_limit: None,
            nonce: Some(nonce),
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

    pub fn with_gas_pricing(mut self, pricing: &GasPricing) -> Self {
        self.gas_limit = Some(pricing.estimated_gas.parse().unwrap_or_default());
        self.gas_price = Some(pricing.gas_price.parse().unwrap_or_default());
        self.max_fee_per_gas = Some(pricing.max_fee_per_gas.parse().unwrap_or_default());
        self.max_priority_fee_per_gas = Some(pricing.max_priority_fee_per_gas.parse().unwrap_or_default());
        self
    }

    pub fn with_nonce(mut self, nonce: u64) -> Self {
        self.nonce = Some(nonce);
        self
    }

    pub fn with_exchange_rate(mut self, rate: f64) -> Self {
        self.exchange_rate = Some(rate);
        self
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

/// General event structure for all transaction lifecycle events
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransactionEvent {
    pub event_id: String,
    pub bundle_id: String,
    pub user_id: String,
    pub event_type: EventType,
    pub leg: Option<TransactionLeg>,
    pub created_at: DateTime<Utc>,
    pub bundle_status: Option<BundleStatus>,
    pub transaction_status: Option<TransactionStatus>, // if leg is present
    pub bundle_snapshot: TransactionBundle
}

impl TransactionEvent {
    pub fn new(
        bundle_id: String,
        user_id: String,
        event_type: EventType,
        leg: Option<TransactionLeg>,
        bundle_status: Option<BundleStatus>,
        transaction_status: Option<TransactionStatus>,
        created_at: DateTime<Utc>,
        bundle_snapshot: TransactionBundle,
    ) -> Self {
        Self {
            event_id: String::default(),
            bundle_id,
            user_id,
            event_type,
            leg,
            bundle_status,
            transaction_status,
            created_at,
            bundle_snapshot,
        }
    }
    pub fn initiate(bundle: TransactionBundle) -> Result<Self, TransactionError> {
        if bundle.status != BundleStatus::Initiated {
            return Err(TransactionError::InvalidTransition(
                format!("Cannot sign from status {:?}", bundle.status), ));
            }

        Ok(TransactionEvent{
            event_id: String::new(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: bundle.user_id.clone(),
            event_type: EventType::Initiate,
            leg: None,
            bundle_status: Some(BundleStatus::Initiated),
            transaction_status: None,
            created_at: Utc::now(),
            bundle_snapshot: bundle.clone(),
        })
    }

    pub async fn on_signed(
        last_event: &TransactionEvent,
        fee_signed: &str,
        main_signed: &str,
        event_store: Arc<TransactionEventManager>,
    ) -> Result<TransactionEvent, TransactionError> {
        if last_event.event_type != EventType::Initiate {
            return Err(TransactionError::InvalidTransition(
                "Signing is only valid after Initiate".into(),
            ));
        }

        if last_event.bundle_status != Some(BundleStatus::Initiated) {
            return Err(TransactionError::InvalidTransition(
                format!("Cannot sign from status {:?}", last_event.bundle_status),
            ));
        }

        let mut bundle = last_event.bundle_snapshot.clone();

        let fee_tx = bundle.fee_tx
            .clone()
            .with_signed_tx(fee_signed)
            .with_status(TransactionStatus::Signed);

        let main_tx = bundle.main_tx
            .clone()
            .with_signed_tx(main_signed)
            .with_status(TransactionStatus::Signed);

        bundle.fee_tx = fee_tx;
        bundle.main_tx = main_tx;
        bundle.status = BundleStatus::Signed;
        bundle.updated_at = Utc::now();

        let mut event = TransactionEvent {
            event_id: String::new(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: last_event.user_id.clone(),
            event_type: EventType::Sign,
            leg: None,
            bundle_status: Some(BundleStatus::Signed),
            transaction_status: None,
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let assigned_event_id = event_store.persist(&event).await?;
        event.event_id = assigned_event_id;

        Ok(event)
    }


    pub async fn on_broadcast(
        last_event: &TransactionEvent,
        tx_hash: H256,
        event_store: Arc<TransactionEventManager>,
    ) -> Result<TransactionEvent, TransactionError> {
        if last_event.event_type != EventType::Confirm && last_event.event_type != EventType::Sign {
            return Err(TransactionError::InvalidTransition(
                "Broadcasting is only valid after signing or confirm".into(),
            ));
        }

        if last_event.bundle_status != Some(BundleStatus::Signed) && last_event.bundle_status != Some(BundleStatus::MainConfirmed) {
            return Err(TransactionError::InvalidTransition(
                format!("Cannot broadcast from status {:?}", last_event.bundle_status),
            ));
        }

        let mut bundle = last_event.bundle_snapshot.clone();
        let hash_str = &format!("{:#x}", tx_hash);
        let (leg, tx) = match (&last_event.event_type, &bundle.status) {
            (EventType::Sign, BundleStatus::Signed) => {
                (TransactionLeg::Main, bundle.main_tx
                                             .clone()
                                             .with_transaction_hash(hash_str)
                                             .with_status(TransactionStatus::Pending))
            }
            (EventType::Confirm, BundleStatus::MainConfirmed) => {
                (TransactionLeg::Fee, bundle.fee_tx
                                            .clone()
                                            .with_transaction_hash(hash_str)
                                            .with_status(TransactionStatus::Pending))
            }
            _ => {
                warn!("🚫 Not a broadcastable state: event_type={:?}, bundle_status={:?}",
                    &last_event.event_type, &bundle.status);
                return Err(TransactionError::InvalidTransition(format!("Not a broadcastable state: event_type={:?}, bundle_status={:?}",
                                                                       &last_event.event_type, &bundle.status)));
            }
        };

        match leg {
            TransactionLeg::Main => bundle.main_tx = tx,
            TransactionLeg::Fee => bundle.fee_tx = tx,
        }

        bundle.updated_at = Utc::now();

        let mut event = TransactionEvent {
            event_id: String::new(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: last_event.user_id.clone(),
            event_type: EventType::Broadcast,
            leg: Some(leg),
            bundle_status: Some(bundle.status.clone()), // no status update on the bundle
            transaction_status: None,
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let assigned_event_id = event_store.persist(&event).await?;
        event.event_id = assigned_event_id;

        Ok(event)
    }

    pub async fn on_fail(
        last_event: &TransactionEvent,
        leg: TransactionLeg,
        event_store: Arc<TransactionEventManager>,
    ) -> Result<TransactionEvent, TransactionError> {
        let mut bundle = last_event.bundle_snapshot.clone();

        match leg {
            TransactionLeg::Fee => {
                bundle.fee_tx = bundle.fee_tx.clone().with_status(TransactionStatus::Failed);
            }
            TransactionLeg::Main => {
                bundle.main_tx = bundle.main_tx.clone().with_status(TransactionStatus::Failed);
            }
        }

        bundle.status = BundleStatus::Failed;
        bundle.updated_at = Utc::now();

        let mut event = TransactionEvent {
            event_id: String::new(), // Will be assigned after persist
            bundle_id: bundle.bundle_id.clone(),
            user_id: last_event.user_id.clone(),
            event_type: EventType::Fail,
            leg: Some(leg),
            bundle_status: Some(BundleStatus::Failed),
            transaction_status: Some(TransactionStatus::Failed),
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let assigned_id = event_store.persist(&event).await?;
        event.event_id = assigned_id;

        Ok(event)
    }

    pub async fn on_error(
        last_event: &TransactionEvent,
        leg: TransactionLeg,
        event_store: Arc<TransactionEventManager>,
    ) -> Result<TransactionEvent, TransactionError> {
        let mut bundle = last_event.bundle_snapshot.clone();

        match leg {
            TransactionLeg::Fee => {
                bundle.fee_tx = bundle.fee_tx.clone().with_status(TransactionStatus::Error);
            }
            TransactionLeg::Main => {
                bundle.main_tx = bundle.main_tx.clone().with_status(TransactionStatus::Error);
            }
        }

        bundle.status = BundleStatus::Errored;
        bundle.updated_at = Utc::now();

        let mut event = TransactionEvent {
            event_id: String::new(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: last_event.user_id.clone(),
            event_type: EventType::Error,
            leg: Some(leg),
            bundle_status: Some(BundleStatus::Errored),
            transaction_status: Some(TransactionStatus::Error),
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let assigned_id = event_store.persist(&event).await?;
        event.event_id = assigned_id;

        Ok(event)
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

/// Detailed information about sender and recipient
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartyDetails {
    pub user_id: String,
    pub name: String,
    pub wallet: String,
}

impl Default for PartyDetails {
    fn default() -> Self {
        Self {
            user_id: Uuid::default().to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Incoming => write!(f, "incoming"),
            Direction::Outgoing => write!(f, "outgoing"),
        }
    }
}

impl FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "incoming" => Ok(Direction::Incoming),
            "outgoing" => Ok(Direction::Outgoing),
            _ => Err(format!("Invalid direction: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionHistoryItem {
    pub bundle_id: String,

    pub direction: Direction, // Incoming or Outgoing
    pub status: TransactionStatus,

    pub amount: f64,
    pub token: String,

    pub counterparty: PartyDetails,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,

    pub timestamp: String, // ISO8601, e.g., "2025-04-23T12:01:00Z"
}

impl TransactionHistoryItem {
    pub fn from_event_and_user(
        event: &TransactionEvent,
        current_user_id: &str,
    ) -> Option<Self> {
        let bundle = &event.bundle_snapshot;
        let metadata = bundle.metadata.as_ref()?;

        // Safely unwrap both sides
        let sender = metadata.sender.as_ref()?;
        let recipient = metadata.recipient.as_ref()?;

        let (direction, counterparty) = if sender.user_id == current_user_id {
            (Direction::Outgoing, recipient.clone())
        } else if recipient.user_id == current_user_id {
            (Direction::Incoming, sender.clone())
        } else {
            return None; // Not relevant to this user
        };

        Some(TransactionHistoryItem {
            bundle_id: bundle.bundle_id.clone(),
            direction,
            status: match event.bundle_status {
                Some(BundleStatus::Initiated) => TransactionStatus::Created,
                Some(BundleStatus::Signed) => TransactionStatus::Signed,
                Some(BundleStatus::MainConfirmed) | Some(BundleStatus::Completed) => TransactionStatus::Confirmed,
                Some(BundleStatus::Failed) => TransactionStatus::Failed,
                Some(BundleStatus::Cancelled) => TransactionStatus::Cancelled,
                Some(BundleStatus::Errored) => TransactionStatus::Error,
                None => TransactionStatus::Created,
            },
            amount: bundle.main_tx.transaction_value as f64 / 1e18, // ETH conversion (18 decimals)
            token: bundle.main_tx.token_type.to_string(),
            tx_hash: bundle.main_tx.transaction_hash.clone(),
            message: metadata.message.clone(),
            timestamp: event.created_at.to_rfc3339(),
            counterparty,
        })
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::utilities::test::{get_cognito_client_with_assumed_role, get_dynamodb_client_with_assumed_role};
    use std::sync::Arc;
    use ethers_core::types::H256;
    use crate::utilities::config;
    use crate::utilities::config::get_transaction_event_table;

    static SIGNED_TX: &str = "0xf86b...";

    #[test]
    fn test_deserialize_transaction_request() {
        config::init();
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
        "gas_pricing": {
            "estimated_gas": "21000",
            "gas_price": "1000000000",
            "max_fee_per_gas": "1000000000",
            "max_priority_fee_per_gas": "1000000000"
        }
    });

        let parsed: Result<TransactionRequest, _> = serde_json::from_value(raw);

        match parsed {
            Ok(req) => {
                assert_eq!(req.fiat_currency_code, "GBP");
                assert_eq!(req.token_type, TokenType::ETH);
                assert!(req.gas_pricing.is_some());
                assert!(req.gas_estimate.is_none());
            }
            Err(e) => {
                panic!("Deserialization failed: {}", e);
            }
        }
    }

    #[test]
    fn test_tx_hash_derives_correctly() {
        config::init();
        let raw = json!({
            "transaction_id": "3e38a355-f26f-4ac2-ac55-1edb5bcfd09f",
            "user_id": "112527246877271240195",
            "sender_address": "0xe006487c4CEC454574b6C9A9F79fF8A5DEe636A0",
            "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
            "transaction_value": 1000,
            "token_type": "ETH",
            "status": "Pending",
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

    #[tokio::test]
    async fn test_transaction_bundle_from_request_creates_expected_bundle() {
        config::init();
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
        "gas_pricing": {
            "estimated_gas": "21000",
            "gas_price": "1000000000",
            "max_fee_per_gas": "1000000000",
            "max_priority_fee_per_gas": "1000000000"
        }
    });

        let request: TransactionRequest = serde_json::from_value(raw).expect("should deserialize request");
        let cognito = get_cognito_client_with_assumed_role().await;
        let dynamo = get_dynamodb_client_with_assumed_role().await;

        let bundle = TransactionBundle::from_request(
            "112527246877271240195".to_string(),
            request,
            &cognito.unwrap(),
            &dynamo,
        ).await.expect("should create bundle");

        // Basic assertions
        assert_eq!(bundle.status, BundleStatus::Initiated);
        let fee_tx = bundle.fee_tx.clone();
        let main_tx = bundle.main_tx.clone();

        assert_eq!(fee_tx.sender_address, "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0");
        assert_eq!(main_tx.recipient_address, "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8");

        let metadata = bundle.metadata.as_ref().expect("should contain metadata");
        assert_eq!(metadata.display_currency, "GBP");
        assert_eq!(metadata.expected_currency_amount, 5000);
        assert_eq!(metadata.gas_pricing.gas_price, "1000000000");
    }


    fn test_request_json() -> serde_json::Value {
        config::init();
        json!({
        "sender_address": "0xe006487c4cec454574b6c9a9f79ff8a5dee636a0",
        "recipient_address": "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8",
        "fiat_value": 5000,
        "fiat_currency_code": "GBP",
        "transaction_value": "1000000000000000",
        "token_type": "ETH",
        "message": "test",
        "exchange_rate": 2300.0,
        "network_fee": "21000000000000",
        "service_fee": "10000000000000",
        "gas_pricing": {
            "estimated_gas": "21000",
            "gas_price": "1000000000",
            "max_fee_per_gas": "1000000000",
            "max_priority_fee_per_gas": "1000000000"
        }
    })
    }

    #[tokio::test]
    async fn minimal_valid_transactionrequest() {
        config::init();
        let request: TransactionRequest = serde_json::from_value(test_request_json()).unwrap();
        let cognito = get_cognito_client_with_assumed_role().await.unwrap();
        let dynamo = get_dynamodb_client_with_assumed_role().await;

        let bundle = TransactionBundle::from_request("112527246877271240195".into(), request, &cognito, &dynamo).await.unwrap();

        assert_eq!(bundle.status, BundleStatus::Initiated);
        assert_eq!(bundle.metadata.as_ref().unwrap().display_currency, "GBP");
    }

    #[tokio::test]
    async fn missing_gas_pricing() {
        config::init();
        let mut invalid_request = test_request_json();
        invalid_request.as_object_mut().unwrap().remove("gas_pricing");

        let request: TransactionRequest = serde_json::from_value(invalid_request).unwrap();
        let cognito = get_cognito_client_with_assumed_role().await.unwrap();
        let dynamo = get_dynamodb_client_with_assumed_role().await;

        let result = TransactionBundle::from_request("112527246877271240195".into(), request, &cognito, &dynamo).await;
        assert!(matches!(result, Err(TransactionError::MissingGasEstimate)));
    }

    #[tokio::test]
    async fn valid_transition_from_initiate() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0);
        let bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);
        let event = TransactionEvent::initiate(bundle).unwrap();
        assert_eq!(event.event_type, EventType::Initiate);
        assert_eq!(event.bundle_status, Some(BundleStatus::Initiated));
    }

    #[tokio::test]
    async fn called_from_non_initiate_event() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0);
        let bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);
        let mut event = TransactionEvent::initiate(bundle).unwrap();
        event.event_type = EventType::Broadcast;

        let dynamo = get_dynamodb_client_with_assumed_role().await;
        let manager = TransactionEventManager::new(Arc::new(dynamo), get_transaction_event_table());
        let result = TransactionEvent::on_signed(&event, SIGNED_TX, SIGNED_TX, manager).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn broadcast_main_leg_from_sign_signed() {
        config::init();
        let request: TransactionRequest = serde_json::from_value(test_request_json()).unwrap();
        let cognito = get_cognito_client_with_assumed_role().await.unwrap();
        let dynamo = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let bundle = TransactionBundle::from_request("112527246877271240195".into(), request, &cognito, &dynamo).await.unwrap();
        let event = TransactionEvent::initiate(bundle.clone()).unwrap();

        let manager = TransactionEventManager::new(dynamo.clone(), get_transaction_event_table());
        let signed_event = TransactionEvent::on_signed(&event, SIGNED_TX, SIGNED_TX, Arc::clone(&manager)).await.unwrap();
        let broadcasted = TransactionEvent::on_broadcast(&signed_event, H256::zero(), manager).await.unwrap();
        assert_eq!(broadcasted.event_type, EventType::Broadcast);
    }

    #[tokio::test]
    async fn broadcast_fee_leg_from_confirm_mainconfirmed() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0)
            .with_status(TransactionStatus::Confirmed);
        let mut bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);
        bundle.status = BundleStatus::MainConfirmed;

        let event = TransactionEvent {
            event_id: "id".into(),
            bundle_id: "bid".into(),
            user_id: "user".into(),
            event_type: EventType::Confirm,
            leg: None,
            bundle_status: Some(BundleStatus::MainConfirmed),
            transaction_status: Some(TransactionStatus::Confirmed),
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let dynamo = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let manager = TransactionEventManager::new(dynamo, get_transaction_event_table());
        let broadcasted = TransactionEvent::on_broadcast(&event, H256::zero(), manager).await.unwrap();
        assert_eq!(broadcasted.event_type, EventType::Broadcast);
    }

    #[tokio::test]
    async fn invalid_event_state_combination() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0);
        let bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);

        let event = TransactionEvent {
            event_id: "e".into(),
            bundle_id: "b".into(),
            user_id: "user".into(),
            event_type: EventType::Error,
            leg: None,
            bundle_status: Some(BundleStatus::Errored),
            transaction_status: None,
            created_at: Utc::now(),
            bundle_snapshot: bundle,
        };

        let dynamo = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let manager = TransactionEventManager::new(dynamo, get_transaction_event_table());
        let result = TransactionEvent::on_broadcast(&event, H256::zero(), manager).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn marks_leg_and_bundle_as_failed() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0);
        let bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);
        let event = TransactionEvent::initiate(bundle).unwrap();

        let dynamo = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let manager = TransactionEventManager::new(dynamo, get_transaction_event_table());
        let failed = TransactionEvent::on_fail(&event, TransactionLeg::Main, manager).await.unwrap();
        assert_eq!(failed.bundle_status, Some(BundleStatus::Failed));
    }

    #[tokio::test]
    async fn marks_leg_and_bundle_as_errored() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000, TokenType::ETH, 100, "GBP".into(), 0);
        let bundle = TransactionBundle::new("user".into(), tx.clone(), tx.clone(), None);
        let event = TransactionEvent::initiate(bundle).unwrap();

        let dynamo = Arc::new(get_dynamodb_client_with_assumed_role().await);
        let manager = TransactionEventManager::new(dynamo, get_transaction_event_table());
        let errored = TransactionEvent::on_error(&event, TransactionLeg::Fee, manager).await.unwrap();
        assert_eq!(errored.bundle_status, Some(BundleStatus::Errored));
    }

    #[tokio::test]
    async fn create_from_transaction_with_complete_gas_data() {
        config::init();
        let tx = Transaction::new("user".into(), "from".into(), "to".into(), 1000000, TokenType::ETH, 100, "GBP".into(), 7)
            .with_gas_pricing(&GasPricing {
                estimated_gas: "21000".into(),
                gas_price: "1000000000".into(),
                max_fee_per_gas: "1000000000".into(),
                max_priority_fee_per_gas: "0".into()
            });

        let unsigned = UnsignedTransaction::from(&tx);
        assert_eq!(unsigned.nonce, "7");
        assert_eq!(unsigned.token_decimals, 18);
    }

    #[test]
    fn test_transaction_history_item_from_event_and_user() {
        use chrono::Utc;
        config::init();

        let sender = PartyDetails {
            user_id: "george123".into(),
            name: "George Michael".into(),
            wallet: "0xgeorge".into(),
        };

        let recipient = PartyDetails {
            user_id: "andrew456".into(),
            name: "Andrew Ridgeley".into(),
            wallet: "0xandrew".into(),
        };

        let metadata = BundleMetadata {
            display_currency: "GBP".into(),
            expected_currency_amount: 2000,
            message: Some("Thanks for the pizza!".into()),
            sender: Some(sender.clone()),
            recipient: Some(recipient.clone()),
            app_version: None,
            location: None,
            service_fee: 0,
            network_fee: 0,
            exchange_rate: 2300.0,
            gas_pricing: GasPricing::default(),
        };

        let main_tx = Transaction::new(
            sender.user_id.clone(),
            sender.wallet.clone(),
            recipient.wallet.clone(),
            1000000000000000000,
            TokenType::ETH,
            2000,
            "GBP".into(),
            1,
        ).with_transaction_hash("0xabc123");

        let fee_tx = main_tx.clone(); // not relevant to this test

        let bundle = TransactionBundle {
            bundle_id: "bundle-xyz".into(),
            user_id: sender.user_id.clone(),
            status: BundleStatus::Completed,
            fee_tx,
            main_tx,
            metadata: Some(metadata),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let event = TransactionEvent {
            event_id: "event-1".into(),
            bundle_id: bundle.bundle_id.clone(),
            user_id: sender.user_id.clone(),
            event_type: EventType::Confirm,
            leg: Some(TransactionLeg::Main),
            created_at: Utc::now(),
            bundle_status: Some(BundleStatus::Completed),
            transaction_status: Some(TransactionStatus::Confirmed),
            bundle_snapshot: bundle,
        };

        let item = TransactionHistoryItem::from_event_and_user(&event, &sender.user_id)
            .expect("should return a valid projection");

        assert_eq!(item.bundle_id, "bundle-xyz");
        assert_eq!(item.direction, Direction::Outgoing);
        assert_eq!(item.counterparty.user_id, "andrew456");
        assert_eq!(item.status, TransactionStatus::Confirmed);
        assert_eq!(item.token, "ETH");
        assert_eq!(item.tx_hash.as_deref(), Some("0xabc123"));
        assert_eq!(item.message.as_deref(), Some("Thanks for the pizza!"));
    }


}
