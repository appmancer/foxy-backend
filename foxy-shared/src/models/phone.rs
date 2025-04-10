use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhoneNumber{
    pub number: String,
    pub countrycode: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PhoneCheckRequest {
    pub phone_numbers: Vec<String>,
    pub country_code: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PhoneCheckResponse {
    pub phone_number: String,
    pub wallet_address: String,
}

//tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phone_number() {
        let phone_number = PhoneNumber {
            number: "1234567890".to_string(),
            countrycode: "1".to_string(),
        };
        assert_eq!(phone_number.number, "1234567890");
        assert_eq!(phone_number.countrycode, "1");
    }
}