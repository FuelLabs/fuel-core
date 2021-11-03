# Stage 1: Build
FROM rust:1.56 as builder

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    clang \
    libclang-dev \
    libssl-dev \
    && mkdir -p ~/.ssh && ssh-keyscan github.com >> ~/.ssh/known_hosts \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build/

COPY . .

RUN --mount=type=ssh cargo build --release

# Stage 2: Run
FROM ubuntu:20.04 as run

ARG IP=0.0.0.0
ARG PORT=4000
ARG DB_PATH=./mnt/db/

ENV IP="${IP}"
ENV PORT="${PORT}"
ENV DB_PATH="${DB_PATH}"

# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /root/

COPY --from=builder /build/target/release/fuel-core .
COPY --from=builder /build/target/release/fuel-core.d .

# https://stackoverflow.com/a/44671685
# https://stackoverflow.com/a/40454758
# hadolint ignore=DL3025
CMD exec ./fuel-core --ip ${IP} --port ${PORT} --db-path ${DB_PATH}

EXPOSE ${PORT}