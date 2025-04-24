use http::StatusCode;
use lambda_http::{Body, Request, Response};
use lambda_http::RequestExt;
use crate::endpoints::{test, wallet, status, phone, auth, transactions, keys};
use foxy_shared::utilities::responses::{success_response, response_with_code};
use foxy_shared::utilities::requests::extract_body;

const GET: &str = "GET";
const POST: &str = "POST";

pub async fn handle_lambda(event: Request) -> Result<Response<Body>, lambda_http::Error> {
    let raw_path = event.raw_http_path();
    let path = raw_path.strip_prefix("/dev")
        .or_else(|| raw_path.strip_prefix("/prod"))
        .unwrap_or(&raw_path);

    log::info!("Received request for path: {}", path);
    let event_body = extract_body(&event);
    log::info!("Received request {:?}", event_body);

    match (event.method().as_str(), path) {
        //Monitor
        (GET, "/test") => success_response(test::handle().await),
        (GET, "/status") => success_response(status::handle().await),

        //Authz
        (POST, "/auth/validate") => auth::validate::handler(event_body).await,
        (POST, "/auth/refresh") => auth::refresh::handler(event_body).await,

        //Encryption
        (POST, "/derive-key") => keys::handler(event, event_body).await,

        //Wallet
        (POST, "/wallet/create") => wallet::create::handler(event, event_body).await,
        (GET, "/wallet/fetch") => wallet::fetch::handler(event).await,
        (GET, "/wallet/balance") => wallet::balance::handler(event).await,

        //User
        (POST, "/phone/verify") => phone::save_number::handler(event, event_body).await,
        (POST, "/phone/checkfoxyusers") => phone::check_numbers::handler(event, event_body).await,

        //Transaction
        (POST, "/transactions/initiate") => transactions::initiate::handler(event, event_body).await,
        (POST, "/transactions/estimate") => transactions::estimate::handler(event, event_body).await,
        (POST, "/transactions/commit") => transactions::commit::handler(event, event_body).await,
        (GET, "/transactions/recent") => transactions::history::handler(event, event_body).await,
        (POST, "/transactions/recent") => transactions::history::handler(event, event_body).await,
        (GET, _) if path.starts_with("/transactions/") => {
            let id = path.trim_start_matches("/transactions/").to_string();
            transactions::single::handler(event, &id).await
        }

        //Not found
        _ => response_with_code("Not Found", StatusCode::NOT_FOUND),
    }
}

