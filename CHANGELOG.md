# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

Description of the upcoming release here.

### Added

- [#1355](https://github.com/FuelLabs/fuel-core/pull/1355): Added new metrics related to block importing, such as tps, sync delays etc
- [#1339](https://github.com/FuelLabs/fuel-core/pull/1339): Adds `baseAssetId` to `FeeParameters` in the GraphQL API.
- [#1331](https://github.com/FuelLabs/fuel-core/pull/1331): Add peer reputation reporting to block import code
- [#1324](https://github.com/FuelLabs/fuel-core/pull/1324): Added pyroscope profiling to fuel-core, intended to be used by a secondary docker image that has debug symbols enabled.
- [#1309](https://github.com/FuelLabs/fuel-core/pull/1309): Add documentation for running debug builds with CLion and Visual Studio Code.  
- [#1308](https://github.com/FuelLabs/fuel-core/pull/1308): Add support for loading .env files when compiling with the `env` feature. This allows users to conveniently supply CLI arguments in a secure and IDE-agnostic way. 
- [#1304](https://github.com/FuelLabs/fuel-core/pull/1304): Implemented `submit_and_await_commit_with_receipts` method for `FuelClient`.
- [#1286](https://github.com/FuelLabs/fuel-core/pull/1286): Include readable names for test cases where missing.
- [#1274](https://github.com/FuelLabs/fuel-core/pull/1274): Added tests to benchmark block synchronization.
- [#1263](https://github.com/FuelLabs/fuel-core/pull/1263): Add gas benchmarks for `ED19` and `ECR1` instructions.

### Changed

- [#1366](https://github.com/FuelLabs/fuel-core/pull/1366): Improve caching during docker builds in CI by replacing gha
- [#1358](https://github.com/FuelLabs/fuel-core/pull/1358): Upgraded the Rust version used in CI to 1.72.0. Also includes associated Clippy changes.
- [#1318](https://github.com/FuelLabs/fuel-core/pull/1318): Modified block synchronization to use asynchronous task execution when retrieving block headers.
- [#1314](https://github.com/FuelLabs/fuel-core/pull/1314): Removed `types::ConsensusParameters` in favour of `fuel_tx:ConsensusParameters`.
- [#1302](https://github.com/FuelLabs/fuel-core/pull/1302): Removed the usage of flake and building of the bridge contract ABI.
    It simplifies the maintenance and updating of the events, requiring only putting the event definition into the codebase of the relayer.
- [#1293](https://github.com/FuelLabs/fuel-core/issues/1293): Parallelized the `estimate_predicates` endpoint to utilize all available threads.
- [#1270](https://github.com/FuelLabs/fuel-core/pull/1270): Modify the way block headers are retrieved from peers to be done in batches.
- [#1342](https://github.com/FuelLabs/fuel-core/pull/1342): Add error handling for P2P requests to return `None` to requester and log error

### Breaking
- [#1363](https://github.com/FuelLabs/fuel-core/pull/1363): Change message_proof api to take `nonce` instead of `message_id`
- [#1339](https://github.com/FuelLabs/fuel-core/pull/1339): Added a new required field called `base_asset_id` to the `FeeParameters` definition in `ConsensusParameters`, as well as default values for `base_asset_id` in the `beta` and `dev` chainspecs.
- [#1355](https://github.com/FuelLabs/fuel-core/pull/1355): Removed the `metrics` feature flag from the fuel-core crate, and metrics are now included by default.
- [#1322](https://github.com/FuelLabs/fuel-core/pull/1322):
  The `debug` flag is added to the CLI. The flag should be used for local development only. Enabling debug mode:
    - Allows GraphQL Endpoints to arbitrarily advance blocks.
    - Enables debugger GraphQL Endpoints.
    - Allows setting `utxo_validation` to `false`.
- [#1318](https://github.com/FuelLabs/fuel-core/pull/1318): Removed the `--sync-max-header-batch-requests` CLI argument, and renamed `--sync-max-get-txns` to `--sync-block-stream-buffer-size` to better represent the current behavior in the import.
- [#1290](https://github.com/FuelLabs/fuel-core/pull/1290): Standardize CLI args to use `-` instead of `_`.
- [#1279](https://github.com/FuelLabs/fuel-core/pull/1279): Added a new CLI flag to enable the Relayer service `--enable-relayer`, and disabled the Relayer service by default. When supplying the `--enable-relayer` flag, the `--relayer` argument becomes mandatory, and omitting it is an error. Similarly, providing a `--relayer` argument without the `--enable-relayer` flag is an error. Lastly, providing the `--keypair` or `--network` arguments will also produce an error if the `--enable-p2p` flag is not set.
- [#1262](https://github.com/FuelLabs/fuel-core/pull/1262): The `ConsensusParameters` aggregates all configuration data related to the consensus. It contains many fields that are segregated by the usage. The API of some functions was affected to use lesser types instead the whole `ConsensusParameters`. It is a huge breaking change requiring repetitively monotonically updating all places that use the `ConsensusParameters`. But during updating, consider that maybe you can use lesser types. Usage of them may simplify signatures of methods and make them more user-friendly and transparent.

### Removed

#### Breaking
- [#1338](https://github.com/FuelLabs/fuel-core/pull/1338): Updated GraphQL client to use `DependentCost` for `k256`, `mcpi`, `s256`, `scwq`, `swwq` opcodes.
- [#1322](https://github.com/FuelLabs/fuel-core/pull/1322): The `manual_blocks_enabled` flag is removed from the CLI. The analog is a `debug` flag.
