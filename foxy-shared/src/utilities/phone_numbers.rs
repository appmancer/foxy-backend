use phonenumber::{Mode, country::Id};
use sha2::{Digest, Sha256};
use regex::Regex;
use crate::models::errors::PhoneNumberError;

fn clean_phone_number(phone_number: &str) -> String {
    let mut cleaned = phone_number.trim().to_string();

    // Remove national dialing prefix "(0)" in international numbers
    let re = Regex::new(r"\(\s*0\s*\)").unwrap();
    cleaned = re.replace_all(&cleaned, "").to_string();

    // List of country codes that require stripping the leading "0"
    let countries_with_leading_zero = vec![
        // Europe
        "44", "33", "49", "34", "43", "32", "31", "351", "41", "45", "30",
        "385", "381", "40", "420", "36", "421", "387", "359", "353", "383", "382", "389", "48", "90", "380",
        // Africa
        "27", "234", "20", "254", "233", "213", "244", "243", "212", "250", "249", "255", "216", "256", "260", "263",
        // Asia
        "93", "374", "994", "880", "855", "86", "995", "91", "62", "98", "964", "972", "81", "962", "7", "850",
        "82", "996", "856", "961", "60", "976", "95", "977", "92", "63", "94", "963", "886", "992", "66", "90",
        "993", "998", "84", "967",
        // South America
        "54", "55", "56", "52", "51", "58",
        // Australia/Oceania
        "61", "64", "672", "56"
    ];

    // Check if number starts with +<country_code>0
    for &code in &countries_with_leading_zero {
        let pattern = format!(r"^\+{}0", code);
        let re = Regex::new(&pattern).unwrap();
        if re.is_match(&cleaned) {
            cleaned = re.replace(&cleaned, &format!("+{}", code)).to_string();
            break; // Stop after first match
        }
    }

    // Remove non-numeric characters except +
    let re_non_numeric = Regex::new(r"[^\d+]").unwrap();
    cleaned = re_non_numeric.replace_all(&cleaned, "").to_string();

    cleaned
}

pub fn normalize_and_hash(phone_number: &str, default_region: &str) -> Result<String, PhoneNumberError> {
    let cleaned_number = clean_phone_number(phone_number);
    let default_region = default_region.trim();

    // Step 1: Attempt to parse without knowing the region (if phone_number starts with "+")
    let parsed = match cleaned_number.starts_with('+') {
        true => phonenumber::parse(None, cleaned_number),
        false => {
            // Step 2: Otherwise, try parsing with the provided region
            let country = default_region.parse::<Id>()
                .map_err(|_| PhoneNumberError::InvalidCountryCode)?;
            phonenumber::parse(Some(country), cleaned_number)
        }
    }
        .map_err(|err| PhoneNumberError::ParseError(format!("{:?}", err)))?;

    // Step 4: Format to E164
    let formatted = parsed.format().mode(Mode::E164).to_string();

    // Step 5: Hash the normalized number
    let mut hasher = Sha256::new();
    hasher.update(formatted.as_bytes());

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_case(phone_number: &str, region: Option<&str>, expected_hash: &str) {
        let result = normalize_and_hash(phone_number, region.unwrap_or("")); // Handle missing region
        match result {
            Ok(ref actual_hash) if actual_hash == expected_hash => {
                println!("✅ Passed: {} ({:?}) -> {}", phone_number, region, actual_hash);
            }
            Ok(ref actual_hash) => {
                println!(
                    "❌ Hash Mismatch: {} ({:?})\n   Expected: {}\n   Found: {}",
                    phone_number, region, expected_hash, actual_hash
                );
                assert_eq!(actual_hash, expected_hash, "Hash does not match for {} ({:?})", phone_number, region);
            }
            Err(ref err) => {
                println!("❌ Failed: {} ({:?}) - {:?}", phone_number, region, err);
                panic!("Test failed due to error: {:?}", err);
            }
        }
    }

    #[test]
    fn test_north_america() {
        test_case("4155552671", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US local
        test_case("+14155552671", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US E164 (should infer)
        test_case("6045551234", Some("CA"), "b9ec580cdb8730d58e79e61dfe354f657422de57937a718d812d90ecef7652b2"); // Canada local
        test_case("+16045551234", None, "b9ec580cdb8730d58e79e61dfe354f657422de57937a718d812d90ecef7652b2"); // Canada E164 (should infer)
    }

    #[test]
    fn test_europe() {
        test_case("07900123456", Some("GB"), "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK local
        test_case("+447900123456", None, "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK E164 (should infer)
        test_case("01721234567", Some("DE"), "c0d11456dc7a796ada86903986d35e2b46a1f183fef8bc464164a9e1e9a130e4"); // Germany local
        test_case("+491721234567", None, "c0d11456dc7a796ada86903986d35e2b46a1f183fef8bc464164a9e1e9a130e4"); // Germany E164 (should infer)
        test_case("0623456789", Some("FR"), "a80a4b85de9efed37310997f8554f5bdb0a3e550ac044371cf7409fa4fb3402d"); // France local
        test_case("+33623456789", None, "a80a4b85de9efed37310997f8554f5bdb0a3e550ac044371cf7409fa4fb3402d"); // France E164 (should infer)
    }

    #[test]
    fn test_asia() {
        test_case("9876543210", Some("IN"), "f3a47ce5ce3d4ca8ad15225a245b2759022f79489f5c62719b8c9490f7aab90e"); // India local
        test_case("+919876543210", None, "f3a47ce5ce3d4ca8ad15225a245b2759022f79489f5c62719b8c9490f7aab90e"); // India E164 (should infer)
        test_case("09012345678", Some("JP"), "3d06a39d40790f11761295e053029378ae28c4b4f6f301693005e079f3d4ca64"); // Japan local
        test_case("+819012345678", None, "3d06a39d40790f11761295e053029378ae28c4b4f6f301693005e079f3d4ca64"); // Japan E164 (should infer)
        test_case("13800138000", Some("CN"), "ec61f3c620a98bdead8c1f1f0ae747abd1b62a0c2dba4fd4bc22cf0d1d8653e5"); // China local
        test_case("+8613800138000", None, "ec61f3c620a98bdead8c1f1f0ae747abd1b62a0c2dba4fd4bc22cf0d1d8653e5"); // China E164 (should infer)
    }

    #[test]
    fn test_africa() {
        test_case("0821234567", Some("ZA"), "44ecafe03bc4f3fe722ac4b67a6621bf147ea8776efdc1cc7acf2d4bbfe18bf4"); // South Africa local
        test_case("+27821234567", None, "44ecafe03bc4f3fe722ac4b67a6621bf147ea8776efdc1cc7acf2d4bbfe18bf4"); // South Africa E164 (should infer)
        test_case("08021234567", Some("NG"), "f3a7e558ecbf58c42bd15a773f6598f9f52423e7d0534e058f67a1f883a3ca1b"); // Nigeria local
        test_case("+2348021234567", None, "f3a7e558ecbf58c42bd15a773f6598f9f52423e7d0534e058f67a1f883a3ca1b"); // Nigeria E164 (should infer)
    }

    #[test]
    fn test_south_america() {
        test_case("11987654321", Some("BR"), "38225ec3dccec4189659c110ddc4f3dc9c27539850cb6a9ddae31ae03a5cf441"); // Brazil local
        test_case("+5511987654321", None, "38225ec3dccec4189659c110ddc4f3dc9c27539850cb6a9ddae31ae03a5cf441"); // Brazil E164 (should infer)
        test_case("1147654321", Some("AR"), "4d90a91d03434022738291089b84f7a2dfceec7dc5cc4802e710d22d39cce076"); // Argentina local
        test_case("+541147654321", None, "4d90a91d03434022738291089b84f7a2dfceec7dc5cc4802e710d22d39cce076"); // Argentina E164 (should infer)
    }

    #[test]
    fn test_australia_oceania() {
        test_case("0412345678", Some("AU"), "bc65da54a3ddbacfdc93a0400f0a2d78e41c2180c8255015e9616facfe56f58a"); // Australia local
        test_case("+61412345678", None, "bc65da54a3ddbacfdc93a0400f0a2d78e41c2180c8255015e9616facfe56f58a"); // Australia E164 (should infer)
        test_case("0211234567", Some("NZ"), "23249c2df7f87de9cd5af2ffaa3d4c206dcd665a8c008f94b9109ab2de685e0d"); // New Zealand local
        test_case("+64211234567", None, "23249c2df7f87de9cd5af2ffaa3d4c206dcd665a8c008f94b9109ab2de685e0d"); // New Zealand E164 (should infer)
    }

    #[test]
    fn test_missing_country_code() {
        // These cases should infer the country from the number itself
        test_case("+14155552671", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US (should infer)
        test_case("+447900123456", None, "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK (should infer)
        test_case("+919876543210", None, "f3a47ce5ce3d4ca8ad15225a245b2759022f79489f5c62719b8c9490f7aab90e"); // India (should infer)
        test_case("+5511987654321", None, "38225ec3dccec4189659c110ddc4f3dc9c27539850cb6a9ddae31ae03a5cf441"); // Brazil (should infer)
    }

    #[test]
    fn test_invalid_numbers() {
        // Invalid country code
        let result = normalize_and_hash("1234567890", "ZZ");
        assert!(result.is_err(), "Expected error for invalid country code");

        // Invalid number format
        let result = normalize_and_hash("abcdefg", "US");
        assert!(result.is_err(), "Expected error for invalid number format");

        // Missing country code for local number
        let result = normalize_and_hash("987654321", "");
        assert!(result.is_err(), "Expected error for missing region");
    }
    #[test]
    fn test_extreme_formatting() {
        // Whitespace variations
        test_case("  415 555 2671  ", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // Leading/trailing spaces
        test_case("\t4155552671\t", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // Tabs
        test_case("\n+14155552671\n", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // Newlines

        // Common separators
        test_case("(415) 555-2671", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with parentheses
        test_case("415-555-2671", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with dashes
        test_case("415.555.2671", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with dots
        test_case("+1 (415) 555-2671", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US international with formatting

        // European formatting styles
        test_case("07900 123 456", Some("GB"), "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK spacing format
        test_case("(07900) 123 456", Some("GB"), "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK with parentheses
        test_case("+4407900123456", Some("GB"), "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK with parentheses
        test_case("+44 (0) 7900 123 456", None, "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // UK with national prefix

        // Asian number formatting
        test_case("090-1234-5678", Some("JP"), "3d06a39d40790f11761295e053029378ae28c4b4f6f301693005e079f3d4ca64"); // Japan common format
        test_case("+81-90-1234-5678", None, "3d06a39d40790f11761295e053029378ae28c4b4f6f301693005e079f3d4ca64"); // Japan international format

        // Australian formatting
        test_case("(04) 1234 5678", Some("AU"), "bc65da54a3ddbacfdc93a0400f0a2d78e41c2180c8255015e9616facfe56f58a"); // Australia with parentheses
        test_case("04-1234-5678", Some("AU"), "bc65da54a3ddbacfdc93a0400f0a2d78e41c2180c8255015e9616facfe56f58a"); // Australia with dashes

        // African formatting
        test_case("(082) 123-4567", Some("ZA"), "44ecafe03bc4f3fe722ac4b67a6621bf147ea8776efdc1cc7acf2d4bbfe18bf4"); // South Africa common format

        // South American formatting
        test_case("(11) 98765-4321", Some("BR"), "38225ec3dccec4189659c110ddc4f3dc9c27539850cb6a9ddae31ae03a5cf441"); // Brazil with parentheses
        test_case("+55 (11) 98765-4321", None, "38225ec3dccec4189659c110ddc4f3dc9c27539850cb6a9ddae31ae03a5cf441"); // Brazil international format

        // Extreme edge cases
        test_case("  (   415 ) - 555  -  2671  ", Some("US"), "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with excessive whitespace and symbols
        test_case("+1-415-555-2671", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with multiple dashes
        test_case("    +1(415)555.2671    ", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // US with mixed formatting and spaces
    }
    #[test]
    fn test_leading_zero_removal_for_multiple_countries() {
        // UK
        test_case("+4407900123456", None, "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8"); // Expected: +447900123456
        test_case("+44 (0) 7900 123 456", None, "efe63e4050575602fa544bddffead773f576bdfd5ca7f4789cbcf745af5a7ea8");

        // Italy - do not strip leading zero
        test_case("+3903471234567", None, "0ab3955e9819d314d4d4b43af81f2bde9b8666c092d02788ce33117a95739e68"); // Expected: +393471234567

        // France
        test_case("+330623456789", None, "a80a4b85de9efed37310997f8554f5bdb0a3e550ac044371cf7409fa4fb3402d"); // Expected: +33612345678

        // Germany
        test_case("+4901721234567", None, "c0d11456dc7a796ada86903986d35e2b46a1f183fef8bc464164a9e1e9a130e4"); // Expected: +4915123456789

        // South Africa
        test_case("+270821234567", None, "44ecafe03bc4f3fe722ac4b67a6621bf147ea8776efdc1cc7acf2d4bbfe18bf4"); // Expected: +27821234567

        // Sweden
        test_case("+460701234567", None, "e45319e195d27b2ad6b556ea4c5504a8c061d85d0b9cbe5f6aff58216804433e"); // Expected: +46701234567

        // USA - should remain unchanged
        test_case("+14155552671", None, "cb6880e416769253645cb9c6b8989154bf66a56a77fc14c81fb1019663cbb928"); // Expected: +14155552671

        // China - should remain unchanged
        test_case("+8613800138000", None, "ec61f3c620a98bdead8c1f1f0ae747abd1b62a0c2dba4fd4bc22cf0d1d8653e5"); // Expected: +8613800138000
    }

}
