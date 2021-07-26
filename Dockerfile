FROM rust:1.53-alpine as builder

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

RUN apk add --no-cache openssh-client git

RUN mkdir -p -m 0600 ~/.ssh && ssh-keyscan github.com >> ~/.ssh/known_hosts

COPY ./ ./

RUN --mount=type=ssh RUSTFLAGS="-C target-cpu=native" cargo build --release