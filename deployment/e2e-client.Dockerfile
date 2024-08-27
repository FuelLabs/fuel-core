# Stage 1: Build
FROM rust:1.79.0 AS chef
RUN cargo install cargo-chef && rustup target add wasm32-unknown-unknown
WORKDIR /build/
# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

FROM chef as planner
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
COPY --from=planner /build/recipe.json recipe.json
# Build our project dependencies, not our application!
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry/index \
  --mount=type=cache,target=/usr/local/cargo/registry/cache \
  --mount=type=cache,target=/usr/local/cargo/git/db \
  --mount=type=cache,target=/build/target \
    cargo chef cook --release -p fuel-core-e2e-client --features p2p --recipe-path recipe.json
# Up to this point, if our dependency tree stays the same,
# all layers should be cached.
COPY . .
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry/index \
  --mount=type=cache,target=/usr/local/cargo/registry/cache \
  --mount=type=cache,target=/usr/local/cargo/git/db \
  --mount=type=cache,target=/build/target \
    cargo build --release -p fuel-core-e2e-client --features p2p \
    && cp ./target/release/fuel-core-e2e-client /root/fuel-core-e2e-client \
    && cp ./target/release/fuel-core-e2e-client.d /root/fuel-core-e2e-client.d

# Stage 2: Run
FROM ubuntu:22.04 as run

WORKDIR /root/

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /root/fuel-core-e2e-client .
COPY --from=builder /root/fuel-core-e2e-client.d .

CMD exec ./fuel-core-e2e-client
