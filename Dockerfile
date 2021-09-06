# Stage 1: Build
FROM rust:1.53 as builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN mkdir -p -m 0600 ~/.ssh && ssh-keyscan github.com >> ~/.ssh/known_hosts
COPY ./ ./build
WORKDIR /build/
RUN --mount=type=ssh cargo build --release

# Stage 2: Run
FROM ubuntu:18.04 as run

# Supports a default, e.g. ARG PORT=4000
ARG PORT=4000
ENV PORT="${PORT}"

RUN apt-get update
RUN apt-get install -y \
    curl \
    libclang-dev \
    libssl-dev

WORKDIR /root/

COPY --from=builder /build/target/release/fuel-core .
COPY --from=builder /build/target/release/fuel-core.d .

# CMD ./fuel-core --ip 0.0.0.0 --port 4000
ENTRYPOINT ./fuel-core --ip 0.0.0.0 --port $PORT

EXPOSE $PORT