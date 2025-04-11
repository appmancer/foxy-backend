pub mod cognito {
    pub const NAME_FIELD: &str = "name";
    pub const EMAIL_FIELD: &str = "email";
    pub const PHONE_FIELD: &str = "custom:phone_hash";
    pub const WALLET_FIELD: &str = "custom:wallet_address";
    pub const DEFAULT_CURRENCY: &str = "custom:default_currency";
}


pub mod dynamodb {
    pub const PHONE_FIELD: &str = "hashed_phone";
    pub const USER_ID_FIELD: &str = "user_id";
    pub const WALLET_FIELD: &str = "wallet_address";
}
