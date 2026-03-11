# Stage 1: Build
FROM rust:1.93.0-bookworm AS chef
RUN cargo install cargo-chef && rustup target add wasm32-unknown-unknown
WORKDIR /build/
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    && apt-get install -y libclang-dev \
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
    cargo chef cook --release -p fuel-core-e2e-client --recipe-path recipe.json
# Up to this point, if our dependency tree stays the same,
# all layers should be cached.
COPY . .
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry/index \
  --mount=type=cache,target=/usr/local/cargo/registry/cache \
  --mount=type=cache,target=/usr/local/cargo/git/db \
  --mount=type=cache,target=/build/target \
    cargo build --release -p fuel-core-e2e-client \
    && cp ./target/release/fuel-core-e2e-client /root/fuel-core-e2e-client \
    && cp ./target/release/fuel-core-e2e-client.d /root/fuel-core-e2e-client.d

# Stage 2: Run
FROM gcr.io/distroless/cc-debian12:nonroot AS run

WORKDIR /home/nonroot/

COPY --from=builder --chown=nonroot:nonroot /root/fuel-core-e2e-client .
COPY --from=builder --chown=nonroot:nonroot /root/fuel-core-e2e-client.d .

ENTRYPOINT ["/home/nonroot/fuel-core-e2e-client"]
