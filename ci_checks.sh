#!/usr/bin/env bash

#export FUEL_TRACE=1
#export RUST_LOG=info

# The script runs almost all CI checks locally.
#
# Requires installed:
# - Rust `1.79.0`
# - Nightly rust formatter
# - `cargo install cargo-sort`
# - `cargo install cargo-make`
# - `cargo install cargo-insta`
# - `cargo install cargo-nextest`
# - `npm install prettier prettier-plugin-toml`

npx prettier --write "**/Cargo.toml" &&
cargo +nightly fmt --all &&
cargo sort -w --check &&
source .github/workflows/scripts/verify_openssl.sh &&
cargo clippy -p fuel-core-wasm-executor --target wasm32-unknown-unknown --no-default-features &&
cargo clippy --all-targets --all-features &&
cargo clippy --manifest-path version-compatibility/Cargo.toml --workspace &&
cargo doc --all-features --workspace --no-deps &&
cargo check -p fuel-core-types --target wasm32-unknown-unknown --no-default-features --features alloc &&
cargo check -p fuel-core-storage --target wasm32-unknown-unknown --no-default-features --features alloc &&
cargo check -p fuel-core-client --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-chain-config --target wasm32-unknown-unknown --no-default-features &&
cargo check -p fuel-core-executor --target wasm32-unknown-unknown --no-default-features --features alloc &&
cargo make check --all-features --locked &&
cargo make check --locked &&
OVERRIDE_CHAIN_CONFIGS=true cargo test --test integration_tests local_node &&
cargo nextest run --workspace &&
FUEL_ALWAYS_USE_WASM=true cargo nextest run --all-features --workspace &&
cargo nextest run -p fuel-core --no-default-features &&
cargo nextest run -p fuel-core-client --no-default-features &&
cargo nextest run -p fuel-core-chain-config --no-default-features &&
cargo nextest run --manifest-path version-compatibility/Cargo.toml --workspace
