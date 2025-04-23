use lambda_http::{service_fn, Error};
use env_logger;
use log;
use crate::router::handle_lambda;
use foxy_shared::utilities::config;
use tracing_subscriber;

mod router;
mod endpoints;
mod models;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
/*    log::info!("Logger initialized");
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // or DEBUG if you want more
        .with_target(false)
        .without_time()
        .init();*/

    std::panic::set_hook(Box::new(|info| {
        log::error!("Application panicked: {}", info);
    }));

    config::init();
    lambda_http::run(service_fn(handle_lambda)).await?;
    Ok(())
}