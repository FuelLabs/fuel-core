#!/usr/bin/env bash

# The script runs almost all CI checks locally.
#
# Requires installed:
# - Rust `1.75.0`
# - Nightly rust formatter
# - `cargo install cargo-sort`
# - `npm install prettier prettier-plugin-toml`

npx prettier --check "**/Cargo.toml" &&
cargo +nightly fmt --all -- --check &&
cargo sort -w --check &&
source .github/workflows/scripts/verify_openssl.sh &&
cargo clippy --all-targets --all-features &&
cargo doc --all-features --workspace --no-deps &&
cargo make check --locked &&
cargo make check --all-features --locked &&
cargo check -p fuel-core-types --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-storage --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-client --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-chain-config --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-executor --target wasm32-unknown-unknown --no-default-features &&
cargo test --all-features --workspace &&
cargo test -p fuel-core --no-default-features &&
cargo test -p fuel-core-client --no-default-features &&
cargo test -p fuel-core-chain-config --no-default-features &&
cargo test --manifest-path version-compatibility/Cargo.toml --workspace