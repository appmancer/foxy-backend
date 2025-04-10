# Foxy-Lambda

Foxy-Lambda is a serverless application built using Rust, designed to run on AWS Lambda. It includes functionality to handle HTTP requests with customizable paths and integrates with API Gateway. The project demonstrates clean, modular Rust development, comprehensive testing, and an efficient CI/CD pipeline.

## Features

- **Serverless Architecture:** Built for AWS Lambda with Rust and `lambda_http`.
- **Path Routing:** Handles multiple paths with stage prefix stripping (`/dev`, `/prod`).
- **Modular Design:** Each route (`/status`, `/test`) has its own handler for better scalability.
- **Comprehensive Testing:** Includes unit and integration tests.
- **CI/CD Integration:** Automated testing and deployment using GitHub Actions.

## Project Structure

```
foxy-lambda/
├── .github/
│   └── workflows/
│       └── deploy.yaml
├── docs/
│   ├── authz_process.md
│   ├── production_cd_steps.md
│   └── ...
├── infrastructure/
│   ├── cloudformation/
│   │   ├── development.yaml
│   │   ├── production.yaml
│   │   └── ...
│   └── scripts/
│       ├── cleanup_and_deploy.sh
│       └── ...
├── src/
│   ├── auth/
│   │   ├── validate.rs
│   │   └── ...
│   ├── cognito/
│   │   ├── client.rs
│   │   ├── user_management.rs
│   │   ├── password.rs
│   │   └── ...
│   ├── endpoints/
│   │   ├── status.rs
│   │   └── ...
│   ├── utilities/
│   │   ├── config.rs
│   │   ├── token_validation.rs
│   │   ├── token_decoding.rs
│   │   └── ...
│   ├── main.rs
│   └── ...
├── Dockerfile
├── README.md
├── .env
└── ...

```

## Prerequisites

Before you begin, ensure you have the following:

- **Rust** (Stable version)
- **AWS CLI** (Configured with appropriate IAM permissions)
- **Docker** (For building the Lambda binary)
- **GitHub Actions** (For CI/CD integration)
## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/foxy-lambda.git
   cd foxy-lambda
   ```

2. Install Rust and required tools:
   ```bash
   rustup install stable
   rustup default stable
   ```

3. Install the `cargo-lambda` tool:
   ```bash
   cargo install cargo-lambda
   ```

4. Build the project:
   ```bash
   cargo build
   ```
## Usage

### Running Locally
You can simulate the Lambda function locally using the `cargo-lambda` tool:
```bash
cargo lambda invoke --input-file payload.json
```

Where `payload.json` contains:
```json
{
"rawPath": "/dev/status"
}
```

### Running Tests
Run unit and integration tests:
```bash
cargo test
```

### Deploying to AWS
1. Build the Lambda binary:
   ```bash
   docker build -t foxy-lambda-builder -f Dockerfile .
   docker run --rm -v $(pwd):/code foxy-lambda-builder cargo lambda build --release --target x86_64-unknown-linux-musl
   ```

2. Package the binary:
   ```bash
   cp target/lambda/foxy-backend/bootstrap .
   zip -j foxy-backend.zip bootstrap
   ```

3. Deploy using the AWS CLI:
   ```bash
   aws lambda update-function-code \
   --function-name foxy-backend \
   --zip-file fileb://foxy-backend.zip
   ```

4. Test the deployed Lambda function:
   ```bash
   curl https://<your-api-id>.execute-api.<region>.amazonaws.com/<stage>/status
   ```

## Testing

The project includes unit and integration tests:

- **Unit Tests:** Validate individual components.
- **Integration Tests:** Test `handle_lambda` with different HTTP paths.

Run all tests:
```bash
cargo test
```
## Deployment

### Building the Lambda Binary
1. Build the Lambda binary using Docker:
   ```bash
   docker build -t foxy-lambda-builder -f Dockerfile .
   docker run --rm -v $(pwd):/code foxy-lambda-builder cargo lambda build --release --target x86_64-unknown-linux-musl
   ```

### Packaging the Binary
2. Package the binary for deployment:
   ```bash
   cp target/lambda/foxy-backend/bootstrap .
   zip -j foxy-backend.zip bootstrap
   ```

### Deploying with AWS CLI
3. Deploy the binary to AWS Lambda:
   ```bash
   aws lambda update-function-code \
   --function-name foxy-backend \
   --zip-file fileb://foxy-backend.zip
   ```

### Testing the Deployed Lambda
4. Test the deployed Lambda function via API

## CI/CD

The project uses GitHub Actions for CI/CD:
- **Build and Test:** Ensures the code passes all tests before deployment.
- **Deploy:** Automatically deploys the Lambda function upon a successful merge to `main`.

### Workflow File

See `.github/workflows/deploy.yml` for details on the CI/CD pipeline.

## Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository.
2. Create a feature branch:
   ```bash
   git checkout -b feature/your-feature
   ```

3. Commit your changes:
   ```bash
   git commit -m "Add your message"
   ```

4. Push to the branch:
   ``` bash
   git push origin feature/your-feature
   ```

5. Open a pull request.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
