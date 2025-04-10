use lambda_runtime::{service_fn, tracing, Error};
use tracing::info;
use dotenv::dotenv;

mod broadcast_handler;

use broadcast_handler::function_handler;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    if dotenv().is_ok() {
        println!("Loaded .env file");
    } else {
        println!("Failed to load .env file");
    }

    info!("🔥 Starting foxy broadcaster lambda runtime...");
    let handler = service_fn(function_handler);
    lambda_runtime::run(handler).await
}
