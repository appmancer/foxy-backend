
#[cfg(test)]
mod tests {
    use alloy_primitives::U256;
    use dotenv::dotenv;
    use http::StatusCode;
    use reqwest::Client;
    use serde_json::json;
    use foxy_shared::models::wallet::BalanceResponse;
    use foxy_shared::services::authentication::generate_tokens;
    use foxy_shared::utilities::test::get_cognito_client_with_assumed_role;

    #[tokio::test]
    async fn test_checkusers() -> Result<(), Box<dyn std::error::Error>> {
        let _ = dotenv().is_ok();
        let api_url = "http://localhost:9000";
        let url = format!("{}/phone/checkfoxyusers", api_url);
        let client = Client::new();

        let test_user_id = "108298283161988749543"; //Jack
        let cognito_client = get_cognito_client_with_assumed_role().await?;
        let token_result = generate_tokens(&cognito_client, &test_user_id)
            .await
            .expect("Failed to get test token");
        let access_token = token_result.access_token.expect("Access token missing");

        let request_json = json!({
            "phone_numbers": [
                "+447533907498",
                "+447593322921",
                "+447593322922",
                "+447593322923"
            ],
            "country_code": "GB"
        });

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&request_json)
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), StatusCode::OK, "Expected 200 OK");

        // get the response body as text
        let body = response.text().await.expect("Failed to read response body");
        let long_zero: U256 = 0.try_into().unwrap();

        // deserialize into BalanceResponse
        let balance_response: BalanceResponse = serde_json::from_str(&body)
            .expect("Failed to deserialize BalanceResponse");
        assert!(balance_response.balance.len() > 0, "Balance does not exist");
        assert!(balance_response.wei.len() > 0, "Wei does not exist");
        assert_eq!(balance_response.token, "ETH", "Incorrect token");
        assert!(balance_response.balance.parse::<f64>().unwrap() > 0.0, "Balance does not exist");
        assert!(balance_response.wei.parse::<U256>().unwrap() > long_zero, "Wei should be greater than zero");

        Ok(())
    }
}