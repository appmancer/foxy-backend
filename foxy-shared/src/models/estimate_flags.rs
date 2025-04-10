// src/models/transactions/estimate_flags.rs

use bitflags::bitflags;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct EstimateFlags: u32 {
        const SUCCESS = 0b00000001;
        const INSUFFICIENT_FUNDS = 0b00000010;
        const WALLET_NOT_FOUND = 0b00000100;
        const EXCHANGE_RATE_UNAVAILABLE = 0b00001000;
        const SERVICE_FEE_UNAVAILABLE = 0b00010000;
        const INTERNAL_ERROR = 0b00100000;
        const INVALID_OPCODE = 0b01000000;
        const CONTRACT_REVERTED = 0b10000000;
        const EXECUTION_REVERTED = 0b00000001_00000000;
        const SIGNATURE_INVALID = 0b00000010_00000000;
        const GAS_LIMIT_TOO_LOW = 0b00000100_00000000;
        const NONCE_ERROR = 0b00001000_00000000;
        const RATE_LIMITED = 0b00010000_00000000;
        const QUOTA_EXCEEDED = 0b00100000_00000000;
        const RPC_AUTHENTICATION_FAILED = 0b01000000_00000000;
    }
}

impl Default for EstimateFlags {
    fn default() -> Self {
        EstimateFlags::empty()
    }
}

pub fn serialize_flags_as_strings<S>(flags: &EstimateFlags, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(None)?;
    for flag in EstimateFlags::all().iter() {
        if flags.contains(flag) {
            let label = match flag {
                EstimateFlags::SUCCESS => "SUCCESS",
                EstimateFlags::INSUFFICIENT_FUNDS => "INSUFFICIENT_FUNDS",
                EstimateFlags::WALLET_NOT_FOUND => "WALLET_NOT_FOUND",
                EstimateFlags::EXCHANGE_RATE_UNAVAILABLE => "EXCHANGE_RATE_UNAVAILABLE",
                EstimateFlags::SERVICE_FEE_UNAVAILABLE => "SERVICE_FEE_UNAVAILABLE",
                EstimateFlags::INTERNAL_ERROR => "INTERNAL_ERROR",
                EstimateFlags::INVALID_OPCODE => "INVALID_OPCODE",
                EstimateFlags::CONTRACT_REVERTED => "CONTRACT_REVERTED",
                EstimateFlags::EXECUTION_REVERTED => "EXECUTION_REVERTED",
                EstimateFlags::SIGNATURE_INVALID => "SIGNATURE_INVALID",
                EstimateFlags::GAS_LIMIT_TOO_LOW => "GAS_LIMIT_TOO_LOW",
                EstimateFlags::NONCE_ERROR => "NONCE_ERROR",
                EstimateFlags::RATE_LIMITED => "RATE_LIMITED",
                EstimateFlags::QUOTA_EXCEEDED => "QUOTA_EXCEEDED",
                EstimateFlags::RPC_AUTHENTICATION_FAILED => "RPC_AUTHENTICATION_FAILED",
                _ => "UNKNOWN",
            };
            seq.serialize_element(label)?;
        }
    }
    seq.end()
}

#[cfg(test)]
mod flag_tests {
    use super::*;

    #[test]
    fn test_flag_combination() {
        let flags = EstimateFlags::SUCCESS | EstimateFlags::INSUFFICIENT_FUNDS;

        assert!(flags.contains(EstimateFlags::SUCCESS));
        assert!(flags.contains(EstimateFlags::INSUFFICIENT_FUNDS));
        assert!(!flags.contains(EstimateFlags::INVALID_OPCODE));
    }

    #[test]
    fn test_flag_serialization() {
        let flags = EstimateFlags::SUCCESS | EstimateFlags::RATE_LIMITED;
        let json = serde_json::to_string(&flags).expect("Serialization failed");
        println!("Serialized flags: {}", json);

        // Should be a number like 0b00010001 or 17
        assert!(json.contains("17") || json.contains("SUCCESS")); // if custom serialization added
    }
}
