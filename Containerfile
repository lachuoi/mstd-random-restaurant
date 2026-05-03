# Stage 1: Build the WASM component
FROM --platform=$BUILDPLATFORM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Set up the target
RUN rustup target add wasm32-wasip2

# Install cargo-component
RUN cargo install cargo-component --locked

WORKDIR /usr/src/app
COPY . .

# Build the release binary
RUN cargo component build --release --target wasm32-wasip2

RUN chmod +x /usr/src/app/target/wasm32-wasip2/release/mstd-random-cafe.wasm

# Stage 2: Create the WASM OCI image
FROM scratch

# Copy the built WASM component to the root
COPY --from=builder /usr/src/app/target/wasm32-wasip2/release/mstd-random-cafe.wasm /mstd-random-cafe.wasm

# Set the entrypoint to the WASM file.
ENTRYPOINT ["/mstd-random-cafe.wasm"]
