use lambda_http::{service_fn, Error};
use env_logger;
use log;
use crate::router::handle_lambda;
use foxy_shared::utilities::config;

mod router;
mod endpoints;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    log::info!("Logger initialized");

    std::panic::set_hook(Box::new(|info| {
        log::error!("Application panicked: {}", info);
    }));

    config::init();
    lambda_http::run(service_fn(handle_lambda)).await?;
    Ok(())
}