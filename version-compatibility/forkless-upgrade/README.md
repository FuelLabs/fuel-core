# Forkless upgrade tests

This crate tests that state transition functions for all releases of `fuel-core` are backward compatible. 

In addition, we also test that releases are forward-compatible unless we introduce a breaking change in the API.

## Running locally

Build the historical native node explicitly before running the compatibility workspace:

```sh
./version-compatibility/build-historical-node.sh
cargo check --manifest-path version-compatibility/Cargo.toml --workspace --locked
cargo test --manifest-path version-compatibility/Cargo.toml --workspace --locked
```

The setup script works from any working directory and installs exactly `fuel-core-bin`
`0.44.0` with its published lockfile, original default features, and `parquet,p2p`.
It uses Cargo's default release profile and the selected toolchain's native target.
The executable is `version-compatibility/target/historical/v0.44.0/bin/fuel-core`;
intermediate build artifacts are reused from
`version-compatibility/target/historical/build/v0.44.0`. Re-running setup does not
force a reinstall. CI runs this setup only for the compatibility matrix entry and
caches both directories by historical version, toolchain, native target, profile,
features, and setup script.

By default setup uses the active Rust toolchain. To select another already-installed
toolchain, run `FUEL_CORE_V44_TOOLCHAIN=<toolchain> ./version-compatibility/build-historical-node.sh`.
CI selects its pinned `RUST_VERSION`. Historical sources can require an older
toolchain or native build prerequisites even when the current workspace builds;
setup clears inherited `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS` so warnings in old
sources are not promoted to errors.

To use an existing executable instead, export an absolute path before both setup
and tests:

```sh
export FUEL_CORE_V44_BIN=/absolute/path/to/fuel-core
./version-compatibility/build-historical-node.sh
cargo check --manifest-path version-compatibility/Cargo.toml --workspace --locked
cargo test --manifest-path version-compatibility/Cargo.toml --workspace --locked
```

With this override, setup validates that the executable reports `fuel-core 0.44.0`
and skips installation. Supply a native binary built with the same default features
plus `parquet,p2p`. Tests use the override at runtime, or the default installation
path derived from the crate directory; they never install the binary themselves.
Keep the initial `cargo check`: the existing v0.26 WASM executor build needs its
locked dependencies fetched before its offline build.

The v0.44 forward-compatibility node runs as a subprocess and is exercised through
its historical GraphQL client. Its executable has an independent dependency graph,
so the old libp2p stack is not unified with the current node's stack and needs no
test-only vendor patch. The v0.26 backward-compatibility and historical WASM executor
coverage remains in-process; this isolation change applies only to the v0.44 node.
The driver binds historical GraphQL and P2P listeners to loopback. These old
dependencies are intentionally preserved for testing, not suitable for deployment.

## Adding new test

We need to add a new backward compatibility test for each new release. To add tests, we need to duplicate tests that are using the latest `fuel-core` and replace usage of the latest crate with a new release.

## Forward compatibility

If the forward compatibility test fails after your changes, it usually means that the change breaks a WASM API, and the network first must upgrade the binary before performing an upgrade of the network.

In the case of breaking API, we need to remove old tests(usually, we need to create a new test per each release) and write a new test(only one) to track new forward compatibility.

## Updating Forward Compatibility Test

If at any point the state transition function becomes forward incompatible, we need to update 
`latest_state_transition_function_is_forward_compatible_with_v44_binary` to use the latest version of `fuel-core`.

Advancing the historical node baseline does not change which older clients are
supported. Retain their compatibility tests and dependencies unless the client
support policy explicitly retires them; do not upgrade those fixtures merely
because the node baseline advances.

To update the test, we need to:
- Update the historical release in `build-historical-node.sh`, the CI cache key, and the driver's executable path and expected node version.
- Update the client/type dependencies used by the historical node driver, preserving dependencies still needed by older-client tests, and verify the new release's CLI arguments and structured GraphQL startup message.
- Add a new `chain-configurations` entry for the new version
- Copy over the contents of the previous version. i.e. if we are updating from `v36` to `v44`, we should create a new 
`v44` directory and copy over the contents of `v36` to `v44`.
- Update `latest_state_transition_function_is_forward_compatible_with_v44_binary` to use the new `chain-configurations/` directory
    - Create a new const for the configuration path, e.g. `pub const V44_TESTNET_SNAPSHOT: &str = "./chain-configurations/v44";`
    - Update the test to use the new const
    - Update the STF version to be native:
        - "genesis_state_transition_version" in `chain_config.json`
        - "state_transition_version" for the `latest_block` in `state_config.json`
        - Bump the versions in the test asserts. i.e. if the version in the configs is `28`, then the asserts will be `29` and `30` respectively.

### Refreshing the lockfile and verifying

After changing the baseline dependencies, run the compatibility check once without
`--locked` to refresh `version-compatibility/Cargo.lock`. Keep the existing lockfile
so unrelated dependencies remain pinned; do not delete it or run an unrestricted
`cargo update` as part of a baseline bump.

From the repository root:

```sh
./version-compatibility/build-historical-node.sh
cargo check --manifest-path version-compatibility/Cargo.toml --workspace --tests
cargo check --manifest-path version-compatibility/Cargo.toml --workspace --tests --locked
cargo test --manifest-path version-compatibility/Cargo.toml --workspace --locked
```

Review the lockfile changes for the intended dependency updates and removals, and
commit them with the baseline change. The final locked check and test commands
verify that the committed dependency graph supports the updated compatibility
tests without further lockfile changes.
