# Foxy Backend Monorepo

This repository contains the core backend services and shared logic for the Foxy app, implemented in Rust and deployed on AWS Lambda. It supports modular development, serverless deployment, and fast iteration across tightly integrated crates.

## Projects

- `foxy-lambda/` – Primary serverless backend running on AWS Lambda and exposed via API Gateway.
- `foxy-shared/` – Shared types, models, and utilities reused across services.
- `foxy-broadcaster/` – SQS-triggered Lambda for Ethereum transaction broadcasting.

## Features

- **Serverless-first Architecture:** Designed for cost-effective, scalable execution via AWS Lambda.
- **Modular Crate Structure:** Organized by service and shared logic for clarity and reuse.
- **Optimized for Iteration:** Single workspace enables tight dev loop without pushing or publishing crates.
- **CI/CD-Ready:** GitHub Actions support multi-crate builds, tests, and deployment.
- **Ethereum Integration:** Support for Optimism L2, EIP-681-compatible requests, and dual transaction model.
- **Secure Auth:** AWS Cognito-based JWT validation, device fingerprinting, and request signatures.

---

## Project Layout

```
foxy-backend/
├── Cargo.toml                # Workspace definition
├── foxy-lambda/              # Main API gateway Lambda backend
├── foxy-shared/              # Common logic shared between services
├── foxy-broadcaster/         # Transaction broadcaster Lambda
```

Each subfolder is a standalone crate and member of the unified Cargo workspace.

## Prerequisites

- **Rust** (latest stable)
- **Docker** (for building Lambda-compatible binaries)
- **AWS CLI** (configured)
- **cargo-lambda** (local simulation & build tool)

To install:
```bash
rustup install stable
cargo install cargo-lambda
```

---

## Running Locally

```bash
# From repo root:
cargo lambda watch -p foxy-lambda
```

You can also test individual handlers by crafting payloads for `cargo lambda invoke`.

## Testing

```bash
cargo test --workspace
```

---

## Deployment

### Build Lambda Binary
```bash
docker build -t foxy-lambda-builder -f foxy-lambda/Dockerfile .
docker run --rm -v $(pwd):/code foxy-lambda-builder \
  cargo lambda build -p foxy-lambda --release --target x86_64-unknown-linux-musl
```

### Package and Deploy
```bash
cp target/lambda/foxy-lambda/bootstrap bootstrap
zip -j foxy-lambda.zip bootstrap

aws lambda update-function-code \
  --function-name foxy-lambda \
  --zip-file fileb://foxy-lambda.zip
```

The same flow applies to `foxy-broadcaster` with the appropriate crate name.

---

## CI/CD

GitHub Actions is configured to:

- Build and test all crates on PR
- Deploy `foxy-lambda` on merge to `main`
- (Future) Deploy `foxy-broadcaster` when enabled

A matrix strategy ensures only changed crates are built and deployed.

---

## Contributing

1. Fork the repository.
2. Create a feature branch:
```bash
git checkout -b feature/your-feature
```
3. Commit your changes:
```bash
git commit -m "Add your message"
```
4. Push and open a PR.

We value clean code, fast iteration, and forward momentum. Fix forward, don’t revert. :rocket:

---

## License

MIT License. See `LICENSE` for details.


