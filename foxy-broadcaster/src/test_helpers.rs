use ethers_signers::{LocalWallet, Signer};
use std::str::FromStr;
use std::time::Duration;
use ethers_core::types::{Address, U256};
use ethers_core::types::TransactionRequest;
use ethers_providers::{Http, Middleware, Provider};
use foxy_shared::utilities::config;
use ethers_core::types::Bytes;
use ethers_core::types::transaction::eip2718::TypedTransaction;

pub const TEST_SENDER_PRIVATE_KEY: &str = "f47e2803d900f88dd8bdb7ddf538047fae4728814f556e3b9906fe8e0d71080f";
pub const TEST_SENDER_ADDRESS: &str = "0xe006487c4CEC454574b6C9A9F79fF8A5DEe636A0";
pub const TEST_RECIPIENT_ADDRESS: &str = "0xa826d3484625b29dfcbdaee6ca636a1acb439bf8";
pub const TEST_USER_ID: &str = "112527246877271240195";

/// Returns the pre-funded test sender wallet.
///
/// **Private Key**: f47e2803d900f88dd8bdb7ddf538047fae4728814f556e3b9906fe8e0d71080f
/// **Address**: 0xe006487c4cec454574b6c9a9f79ff8a5dee636a0
pub fn funded_sender_wallet() -> LocalWallet {
    // This unwrap is safe because the key is known to be valid.
    LocalWallet::from_str(TEST_SENDER_PRIVATE_KEY)
        .expect("Failed to parse test private key")
}

pub fn test_provider() -> Provider<Http> {
    let url = config::get_test_rpc_url();
    Provider::<Http>::try_from(url)
        .expect("Invalid Infura URL")
        .interval(Duration::from_millis(200)) // avoid rate-limiting
}

pub async fn sign_test_transaction(wallet: &LocalWallet, provider: &Provider<Http>) -> Bytes {
    let sender = wallet.address();
    let recipient: Address = TEST_RECIPIENT_ADDRESS
        .parse()
        .expect("Invalid recipient address");

    let nonce = provider
        .get_transaction_count(sender, None)
        .await
        .expect("Could not get nonce");

    let tx = TransactionRequest::pay(recipient, U256::from(1_000_000_000_000u64))
        .from(sender)
        .nonce(nonce)
        .gas(21_000)
        .gas_price(U256::from(1_000_000)) // ~1 gwei
        .chain_id(11155420u64);

    let typed_tx: TypedTransaction = tx.into();

    // Sign and serialize (RLP encoding)
    let signature = wallet
        .sign_transaction(&typed_tx)
        .await
        .expect("Failed to sign transaction");

    let rlp_encoded = typed_tx.rlp_signed(&signature);
    Bytes::from(rlp_encoded)
}

