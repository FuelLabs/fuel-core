# Fuel Client

[![build](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml/badge.svg)](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/fuel-core?label=latest)](https://crates.io/crates/fuel-core)
[![docs](https://docs.rs/fuel-core/badge.svg)](https://docs.rs/fuel-core/)
[![discord](https://img.shields.io/badge/chat%20on-discord-orange?&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/xfpK4Pe)

Fuel client implementation.

## Contributing

If you are interested in contributing to Fuel, see our [CONTRIBUTING.md](CONTRIBUTING.md) guidelines for coding standards and review process.

Before pushing any changes or creating pull request please run `source ci_checks.sh`.

## Building

##### System Requirements

There are several system requirements including clang.

###### MacOS

```bash
brew update
brew install cmake
brew install protobuf
```

###### Debian

```bash
apt update
apt install -y cmake pkg-config build-essential git clang libclang-dev protobuf-compiler
```

###### Arch

```bash
pacman -Syu --needed --noconfirm cmake gcc pkgconf git clang protobuf-compiler
```

## Building

We recommend using `xtask` to build fuel-core:

```sh
cargo xtask build
```

This will run `cargo build` as well as any other custom build processes we have such as re-generating a GraphQL schema for the client.

## Running

The service can be launched by executing `fuel-core run`. The list of options for running can be accessed via the `help` option:

```console
$ ./target/debug/fuel-core run --help

USAGE:
    fuel-core run [OPTIONS]

OPTIONS:
        --chain <CHAIN_CONFIG>
            Specify either an alias to a built-in configuration or filepath to a JSON file [default:
            local_testnet]
        ...
```

For many development puposes it is useful to have a state that won't persist and the `db-type` option can be set to `in-memory` as in the following example.

#### Example

```console
$ ./target/debug/fuel-core run --db-type in-memory
Jul 12 23:28:47.238  INFO fuel_core: Binding GraphQL provider to 127.0.0.1:4000
```

To disable block production on your local node, set `--poa-instant=false`

#### Example

```console
$ ./target/debug/fuel-core run --poa-instant=false
2023-01-23T02:25:18.787401Z  INFO fuel_core::cli::run: 173: Block production disabled.
```

#### Troubleshooting

##### Publishing

We use [`publish-crates`](https://github.com/katyo/publish-crates) action for automatic publishing of all crates.

If you have problems with publishing, you can troubleshoot it locally with [`act`](https://github.com/nektos/act).

```shell
act release -s GITHUB_TOKEN=<YOUR_GITHUB_TOKEN> -j publish-crates-check --container-architecture linux/amd64 --reuse
```

It requires GitHubToken to do request to the GitHub. You can create it 
with [this](https://docs.github.com/en/enterprise-server@3.4/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token) instruction.

##### Outdated database

If you encounter an error such as

```console
thread 'main' panicked at 'unable to open database: DatabaseError(Error { message: "Invalid argument: Column families not opened: column-11, column-10, column-9, column-8, column-7, column-6, column-5, column-4, column-3, column-2, column-1, column-0" })', fuel-core/src/main.rs:23:66
```

Clear your local database using: `rm -rf ~/.fuel/db`

##### File descriptor limits

On some macOS versions the default file descriptor limit is quite low, which can lead to IO errors with messages like `Too many open files` or even `fatal runtime error: Rust cannot catch foreign exceptions` when RocksDB encounters these issues. Use the following command to increase the open file limit. Note that this only affects the current shell session, so consider adding it to `~/.zshrc`.

```bash
ulimit -n 10240
```

#### Log level

The service relies on the environment variable `RUST_LOG`. For more information, check the [EnvFilter examples](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/struct.EnvFilter.html#examples) crate.

Human logging can be disabled with the environment variable `HUMAN_LOGGING=false`

## Docker & Kubernetes

```sh
# Create Docker Image
docker build -t fuel-core . -f deployment/Dockerfile

# Delete Docker Image
docker image rm fuel-core

# Create Kubernetes Volume, Deployment & Service
kubectl create -f deployment/fuel-core.yml

# Delete Kubernetes Volume, Deployment & Service
kubectl delete -f deployment/fuel-core.yml
```

## GraphQL service

The client functionality is available through a service endpoint that expect GraphQL queries.

#### Transaction executor

The transaction executor currently performs instant block production. Changes are persisted to RocksDB by default.

-   Service endpoint: `/graphql`
-   Schema (available after building): `fuel-client/assets/schema.sdl`

The service expects a mutation defined as `submit` that receives a [Transaction](https://github.com/FuelLabs/fuel-tx) in hex encoded binary format, as [specified here](https://github.com/FuelLabs/fuel-specs/blob/master/specs/protocol/tx_format.md).

##### cURL example

This example will execute a script that represents the following sequence of [ASM](https://github.com/FuelLabs/fuel-asm):

```rs
ADDI(0x10, RegId::ZERO, 0xca),
ADDI(0x11, RegId::ZERO, 0xba),
LOG(0x10, 0x11, RegId::ZERO, RegId::ZERO),
RET(RegId::ONE),
```

```console
$ cargo run --bin fuel-gql-cli -- transaction submit \
"{\"Script\":{\"gas_price\":0,\"gas_limit\":1000000,\"maturity\":0,\"script\":[80,64,0,202,80,68,0,186,51,65,16,0,36,4,0,0],\"script_data\":[],\"inputs\":[],\"outputs\":[],\"witnesses\":[],\"receipts_root\":\"0x6114142d12e0f58cfb8c72c270cd0535944fb1ba763dce83c17e882c482224a2\"}}"
```
