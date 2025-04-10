use serde::Serialize;

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
}

pub async fn handle() -> StatusResponse {
    StatusResponse { status: "OK".to_string() }
}
