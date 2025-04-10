use http::StatusCode;
use lambda_http::{Response, Body};
use serde::Serialize;

pub fn success_response<T: Serialize>(data: T) -> Result<Response<Body>, lambda_http::Error> {
    response_with_code(data, StatusCode::OK)
}

pub fn created_response<T: Serialize>(data: T) -> Result<Response<Body>, lambda_http::Error> {
    response_with_code(data, StatusCode::CREATED)
}

pub fn error_response<T: Serialize>(data: T) -> Result<Response<Body>, lambda_http::Error> {
    response_with_code(data, StatusCode::BAD_REQUEST)
}

pub fn response_with_code<T: Serialize>(data: T, code: StatusCode) -> Result<Response<Body>, lambda_http::Error> {
    let body = serde_json::to_string(&data).map_err(|_| lambda_http::Error::from("Serialization error"))?;
    log::info!("Response Code:{}\nBody: {}", code, body);
    Response::builder()
        .status(code)
        .header("Content-Type", "application/json")
        .body(Body::Text(body))
        .map_err(|e| {
            log::error!("Failed to build response: {:?}", e);
            lambda_http::Error::from("Failed to construct HTTP response")
        })
}
