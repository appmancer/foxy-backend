#[cfg(test)]
mod tests {
    use http::{Method, Uri};
    use lambda_http::{Body, Request, RequestExt};
    use serde_json::json;
    use foxy_lambda::router::handle_lambda;

    #[tokio::test]
    async fn test_lambda_handler_status_path() {
        // Create a request for "/dev/status"
        let mut request = Request::from(http::Request::builder()
            .method(Method::GET)
            .uri(Uri::from_static("/dev/status"))
            .body(Body::Empty)
            .unwrap());

        request = request.with_raw_http_path("/dev/status");

        println!("Sending path: {}", request.raw_http_path());

        let response = handle_lambda(request).await.unwrap();

        // Check status code
        assert_eq!(response.status(), 200);

        // Check body
        if let Body::Text(body) = response.body() {
            let json_body: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(json_body, json!({"status": "OK"}));
        } else {
            panic!("Response body is not text");
        }
    }

    #[tokio::test]
    async fn test_lambda_handler_not_found_path() {
        // Create a request for an unknown path
        let request = Request::from(http::Request::builder()
            .method(Method::GET)
            .uri("/dev/unknown")
            .body(Body::Empty)
            .unwrap());

        let response = handle_lambda(request).await.unwrap();

        // Check status code
        assert_eq!(response.status(), 404);

        // Check body
        if let Body::Text(body) = response.body() {
            let json_body: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(json_body, json!({"error": "Not Found"}));
        } else {
            panic!("Response body is not text");
        }
    }

    #[tokio::test]
    async fn test_lambda_handler_test_path() {
        // Create a request for "/prod/test"
        let mut request = Request::from(http::Request::builder()
            .method(Method::GET)
            .uri(Uri::from_static("/dev/test")) // Ensure raw HTTP path is set
            .body(Body::Empty)
            .unwrap());

        request = request.with_raw_http_path("/dev/test");
        let response = handle_lambda(request).await.unwrap();

        // Check status code
        assert_eq!(response.status(), 200);

        // Add checks for the body based on `test::handle()` output
        if let Body::Text(body) = response.body() {
            let json_body: serde_json::Value = serde_json::from_str(body).unwrap();
            assert!(json_body.is_object()); // Adjust based on `test::handle()` implementation
        } else {
            panic!("Response body is not text");
        }
    }
}


