# Stage 1: Build
FROM rust:1.66 AS chef
RUN cargo install cargo-chef
WORKDIR /build/
# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    lld \
    clang \
    libclang-dev \
    protobuf-compiler \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

FROM chef as planner
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
COPY --from=planner /build/recipe.json recipe.json
# Build our project dependecies, not our application!
RUN cargo chef cook --release --features "production" -p fuel-core-bin --recipe-path recipe.json
# Up to this point, if our dependency tree stays the same,
# all layers should be cached.
COPY . .
RUN cargo build --release --features "production" -p fuel-core-bin

# Stage 2: Run
FROM ubuntu:22.04 as run

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

EXPOSE ${PORT}
EXPOSE ${P2P_PORT}

# https://stackoverflow.com/a/44671685
# https://stackoverflow.com/a/40454758
# hadolint ignore=DL3025
CMD exec ./fuel-core run --ip ${IP} --port ${PORT} --db-path ${DB_PATH}
