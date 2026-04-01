# Repository structure

This guide provides a quick map of the `fuel-core` repository for contributors who need to find the right crate, binary, or support directory before making changes.

## Top-level layout

| Path | Purpose |
|---|---|
| `bin/` | Workspace binaries such as `fuel-core`, `fuel-core-client`, `keygen`, and test-oriented CLI tools. |
| `crates/` | Library crates that implement the core node, shared types, storage, metrics, and service modules. |
| `tests/` | Workspace-level integration tests and shared test helpers. |
| `docs/` | Design notes and contributor-facing documentation. |
| `deployment/` | Container build assets and deployment-oriented documentation. |
| `benches/` | Benchmark crates and benchmark helpers. |
| `version-compatibility/` | Release-compatibility tooling and chain configuration fixtures that are intentionally excluded from the main workspace. |
| `xtask/` | Custom contributor workflows such as build-time automation. |
| `.github/` | CI workflows, automation, and pull request templates. |
| `.changes/` | Changelog fragments used by the release workflow. |
| `ci_checks.sh` | Convenience entrypoint for the repository CI checks described in the README. |

## Workspace members

The root [`Cargo.toml`](../Cargo.toml) defines a Rust workspace with a few broad groups:

- `bin/*`: executable entrypoints, including the main node binary in `bin/fuel-core`
- `crates/fuel-core`: the core crate that wires GraphQL, services, state, and node-facing APIs together
- `crates/services/*`: service implementations such as consensus, gas price, importer, producer, relayer, sync, and txpool
- `crates/{database,storage,types,metrics,provider,client,...}`: shared libraries used across the node and its tooling
- `tests`: the integration-test crate for end-to-end and cross-service behavior
- `xtask`: custom build and maintenance commands used during development

## Where to start for common tasks

| If you want to... | Start here |
|---|---|
| Run or extend the main CLI | `bin/fuel-core/` |
| Trace how the node assembles services | `crates/fuel-core/src/service.rs` and `crates/services/` |
| Work on GraphQL behavior | `crates/fuel-core/src/graphql_api.rs` and related schema/query files in `crates/fuel-core/src/` |
| Modify a specific runtime subsystem | The matching crate under `crates/services/` |
| Update storage or shared domain types | `crates/storage/`, `crates/database/`, and `crates/types/` |
| Add or repair integration coverage | `tests/tests/` and `tests/test-helpers/` |
| Update contributor or operator docs | `docs/`, `deployment/`, `README.md`, or `CONTRIBUTING.md` |
| Adjust custom build flows | `xtask/` |

## Notes on a few commonly visited directories

### `bin/`

The `bin/` directory contains the workspace binaries. The most common entrypoint is `bin/fuel-core/`, which provides the main executable. Other binaries support client usage, key generation, end-to-end testing, or chaos-style testing.

### `crates/`

Most code changes land under `crates/`. A useful rule of thumb is:

- use `crates/fuel-core/` when changing node-level composition or API wiring
- use `crates/services/` when changing a specific subsystem
- use the other top-level crates when the change is a reusable library concern rather than a node assembly concern

### `tests/`

The `tests/` workspace member is the integration surface for behavior that spans crates or services. Shared fixtures and utilities live in `tests/test-helpers/`.

### `version-compatibility/`

This directory is present in the repository but excluded from the main workspace. It is used for compatibility and upgrade-oriented workflows rather than normal day-to-day node development.
