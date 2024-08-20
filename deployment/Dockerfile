# Stage 1: Build
FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx
FROM --platform=$BUILDPLATFORM rust:1.79.0 AS chef

ARG TARGETPLATFORM
RUN cargo install cargo-chef && rustup target add wasm32-unknown-unknown
WORKDIR /build/

COPY --from=xx / /

# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    lld \
    clang \
    libclang-dev \
    && xx-apt-get update  \
    && xx-apt-get install -y libc6-dev g++ binutils \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*


FROM chef AS planner
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder
ARG FEATURES="production"
ARG DEBUG_SYMBOLS=false
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_PROFILE_RELEASE_DEBUG=$DEBUG_SYMBOLS
ENV BUILD_FEATURES=$FEATURES
COPY --from=planner /build/recipe.json recipe.json
RUN echo $CARGO_PROFILE_RELEASE_DEBUG
RUN echo $BUILD_FEATURES
# Build our project dependencies, not our application!
RUN xx-cargo chef cook --release --no-default-features --features "${BUILD_FEATURES}" -p fuel-core-bin --recipe-path recipe.json
# Up to this point, if our dependency tree stays the same,
# all layers should be cached.
COPY . .
# download latest chain-configuration repo after cache to ensure most recent version
RUN git clone --depth=1 https://github.com/FuelLabs/chain-configuration.git /chain-config
# build application
RUN xx-cargo build --release --no-default-features --features "$BUILD_FEATURES" -p fuel-core-bin \
    && xx-verify ./target/$(xx-cargo --print-target-triple)/release/fuel-core \
    && mv ./target/$(xx-cargo --print-target-triple)/release/fuel-core ./target/release/fuel-core \
    && mv ./target/$(xx-cargo --print-target-triple)/release/fuel-core.d ./target/release/fuel-core.d

# Stage 2: Run
FROM ubuntu:22.04 AS run

ARG IP=0.0.0.0
ARG PORT=4000
ARG P2P_PORT=30333
ARG DB_PATH=./mnt/db/

ENV IP="${IP}"
ENV PORT="${PORT}"
ENV DB_PATH="${DB_PATH}"

WORKDIR /root/

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/fuel-core .
COPY --from=builder /build/target/release/fuel-core.d .
COPY --from=builder /chain-config ./config

EXPOSE ${PORT}
EXPOSE ${P2P_PORT}

# https://stackoverflow.com/a/44671685
# https://stackoverflow.com/a/40454758
# hadolint ignore=DL3025
CMD exec ./fuel-core run --ip ${IP} --port ${PORT} --db-path ${DB_PATH}
