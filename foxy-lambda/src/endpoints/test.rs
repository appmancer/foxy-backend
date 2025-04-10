use serde::Serialize;

#[derive(Serialize)]
pub struct TestResponse {
    pub status: String,
}

pub async fn handle() -> TestResponse {
    TestResponse { status: "Hello Sammy!".to_string() }
}
