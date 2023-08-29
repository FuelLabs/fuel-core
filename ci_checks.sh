#!/usr/bin/env bash

# The script runs almost all CI checks locally.
#
# Requires installed:
# - Rust `1.71.0`
# - Nightly rust formatter
# - `cargo install cargo-sort`

cargo +nightly fmt --all -- --check &&
cargo sort -w --check &&
source .github/workflows/scripts/verify_openssl.sh &&
cargo clippy --all-targets --all-features &&
cargo make check --locked &&
cargo make check --all-features --locked &&
cargo test --all-features --workspace &&
cargo test -p fuel-core --no-default-features &&
cargo test -p fuel-core-client --no-default-features &&
cargo test -p fuel-core-chain-config --no-default-features &&
cargo test --manifest-path version-compatibility/Cargo.toml --workspace