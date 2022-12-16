cargo +nightly fmt --all -- --check &&
cargo sort -w --check &&
source .github/workflows/scripts/verify_openssl.sh &&
cargo clippy --all-targets --all-features &&
cargo check --locked &&
cargo test --all-features --workspace &&
cargo test --no-default-features --workspace &&
cargo test --manifest-path version-compatibility/Cargo.toml --workspace