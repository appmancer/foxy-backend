use aws_sdk_sqs::{Client, Error};
use aws_config::meta::region::RegionProviderChain;
use serde_json::json;

pub async fn get_sqs_client() -> Result<Client, Error> {
    // Use default AWS region chain (env var → config file → fallback)
    let region_provider = RegionProviderChain::default_provider().or_else("eu-north-1"); // or your default
    let config = aws_config::from_env().region(region_provider).load().await;
    Ok(Client::new(&config))
}

pub async fn push_to_broadcast_queue(
    sqs_client: &Client,
    queue_url: &str,
    transaction_id: &str,
    user_id: &str,
) -> Result<(), Error> {
    let payload = json!({
        "transaction_id": transaction_id,
        "user_id": user_id
    })
        .to_string();

    sqs_client
        .send_message()
        .queue_url(queue_url)
        .message_body(payload)
        .send()
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_sqs::{Client as SqsClient};
    use dotenv::dotenv;
    use crate::utilities::config::get_broadcast_queue;
    use crate::utilities::test::get_sqs_client_with_assumed_role;

    pub async fn peek_queue(
        sqs_client: &SqsClient,
        queue_url: &str,
    ) -> Vec<(String, String)> {
        let result = sqs_client
            .receive_message()
            .queue_url(queue_url)
            .max_number_of_messages(10)
            .visibility_timeout(0)
            .wait_time_seconds(1)
            .send()
            .await
            .expect("Failed to receive messages");

        // result.messages() is already &[Message]
        result
            .messages()
            .iter()
            .filter_map(|msg| {
                let body = msg.body()?;
                let receipt = msg.receipt_handle()?;
                Some((body.to_string(), receipt.to_string()))
            })
            .collect()
    }

    #[tokio::test]
    async fn test_push_and_peek_broadcast_queue() {
        let _ = dotenv().is_ok();
        let queue_url = get_broadcast_queue();
        let sqs_client = get_sqs_client_with_assumed_role()
            .await
            .expect("Failed to get assumed SQS client");

        let transaction_id = "tx_test_12345";
        let user_id = "user_test_abc";

        // Send to queue
        push_to_broadcast_queue(&sqs_client, &queue_url, transaction_id, user_id)
            .await
            .expect("Failed to push message to queue");

        // Peek messages
        let messages = peek_queue(&sqs_client, &queue_url).await;

        // Find and delete matching message
        let mut found = false;
        for (body, receipt_handle) in messages {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
                let matches = parsed.get("transaction_id") == Some(&serde_json::json!(transaction_id))
                    && parsed.get("user_id") == Some(&serde_json::json!(user_id));

                if matches {
                    found = true;

                    // ✅ Delete the message to keep the queue clean
                    sqs_client
                        .delete_message()
                        .queue_url(&queue_url)
                        .receipt_handle(receipt_handle)
                        .send()
                        .await
                        .expect("Failed to delete message");

                    break;
                }
            }
        }

        assert!(found, "Expected broadcast message not found in queue");
    }
}
