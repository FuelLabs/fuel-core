# Stage 1: Base Setup
FROM rust:1.75.0 AS base

# Install cargo-chef and add the wasm target
RUN cargo install cargo-chef && rustup target add wasm32-unknown-unknown

# Set working directory
WORKDIR /build/

# Install necessary dependencies and clean up
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates \
    && apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Stage 2: Dependency Planning
FROM base as planner

# Set environment variable to force Cargo to use the git CLI for fetching
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Copy the project files to the container
COPY . .

# Generate a recipe for cargo-chef to cache dependencies
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Build Application
FROM base as builder

# Set environment variable to force Cargo to use the git CLI for fetching
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Copy the dependency recipe from the planner stage
COPY --from=planner /build/recipe.json recipe.json

# Build the project dependencies, caching the results
RUN cargo chef cook --release -p fuel-core-e2e-client --features p2p --recipe-path recipe.json

# Copy the remaining project files and build the application
COPY . .

RUN cargo build --release -p fuel-core-e2e-client --features p2p

# Stage 4: Final Runtime Image
FROM ubuntu:22.04 as runtime

# Set working directory
WORKDIR /root/

# Install necessary runtime dependencies and clean up
RUN apt-get update -y && \
    apt-get install -y --no-install-recommends ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary and any associated files from the builder stage
COPY --from=builder /build/target/release/fuel-core-e2e-client .
COPY --from=builder /build/target/release/fuel-core-e2e-client.d .

# Set the entrypoint to the built binary
CMD ["./fuel-core-e2e-client"]
