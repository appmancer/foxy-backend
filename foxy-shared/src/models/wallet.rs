use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BalanceResponse {
    pub balance: String,
    pub token: String,
    pub wei: String,
    pub fiat: FiatBalance
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FiatBalance {
    pub value: String,          // "20.00"
    pub currency: String,       // "GBP"
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WalletCreateResponse {
    pub message: String,
}

#[derive(Serialize, Debug)]
pub struct WalletFetchResponse {
    #[serde(rename = "walletAddress")]
    pub wallet_address: String,
}
