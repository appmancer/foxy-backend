# Use the official cargo-lambda image as the base
FROM ghcr.io/cargo-lambda/cargo-lambda:latest

# Install dependencies
RUN apt-get update && apt-get install -y \
    openssl libssl-dev pkg-config musl musl-dev musl-tools zip build-essential curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Set environment variables for OpenSSL
ENV OPENSSL_DIR=/usr
ENV OPENSSL_INCLUDE_DIR=/usr/include
ENV OPENSSL_LIB_DIR=/usr/lib
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig

# Install Zig to support cargo-zigbuild
RUN curl -L https://ziglang.org/download/0.11.0/zig-linux-x86_64-0.11.0.tar.xz | tar -xJf - -C /usr/local/ --strip-components=1

# Ensure musl target for Rust
RUN rustup target add x86_64-unknown-linux-musl

# Set working directory
WORKDIR /code

