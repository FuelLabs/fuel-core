# Fuel Client

[![build](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml/badge.svg)](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/fuel-core?label=latest)](https://crates.io/crates/fuel-core)
[![docs](https://docs.rs/fuel-core/badge.svg)](https://docs.rs/fuel-core/)
[![discord](https://img.shields.io/badge/chat%20on-discord-orange?&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/xfpK4Pe)

Fuel client implementation.

## Contributing

If you are interested in contributing to Fuel, see our [CONTRIBUTING.md](CONTRIBUTING.md) guidelines for coding standards and review process.

## Building

##### System Requirements

There are several system requirements including clang.

###### MacOS
```bash
brew update
brew install cmake
```

###### Debian
```bash
apt update
apt install -y cmake pkg-config build-essential git clang libclang-dev
```

###### Arch
```bash 
pacman -Syu --needed --noconfirm cmake gcc pkgconf git clang
```

## Building 

We recommend using `xtask` to build fuel-core:

```
cargo xtask build
```

This will run `cargo build` as well as any other custom build processes we have such as re-generating a GraphQL schema for the client.

## Running

The service can listen to an arbitrary socket, as specified in the help command:

```
$ ./target/debug/fuel-core --help
fuel-core 0.1.0

USAGE:
    fuel-core [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --ip <ip>              [default: 127.0.0.1]
        --port <port>          [default: 4000]
        --db-path <file path>  [default: None]
```

#### Example

```
$ ./target/debug/fuel-core --ip 127.0.0.1 --port 4000
Jul 12 23:28:47.238  INFO fuel_core: Binding GraphQL provider to 127.0.0.1:4000
```

#### Troubleshooting

If you encounter an error such as
```
thread 'main' panicked at 'unable to open database: DatabaseError(Error { message: "Invalid argument: Column families not opened: column-11, column-10, column-9, column-8, column-7, column-6, column-5, column-4, column-3, column-2, column-1, column-0" })', fuel-core/src/main.rs:23:66
```
Clear your local database using: `rm -rf ~/.fuel/db`

#### Log level

The service relies on the environment variable `RUST_LOG`. For more information, check the [EnvFilter examples](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/struct.EnvFilter.html#examples) crate.

Human logging can be disabled with the environment variable `HUMAN_LOGGING=false`

## Docker & Kubernetes
```
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

* Service endpoint: `/graphql`
* Schema (available after building): `fuel-client/assets/schema.sdl`

The service expects a mutation defined as `submit` that receives a [Transaction](https://github.com/FuelLabs/fuel-tx) in hex encoded binary format, as [specified here](https://github.com/FuelLabs/fuel-specs/blob/master/specs/protocol/tx_format.md).

##### cURL example

This example will execute a script that represents the following sequence of [ASM](https://github.com/FuelLabs/fuel-asm):

```
ADDI(0x10, REG_ZERO, 0xca),
ADDI(0x11, REG_ZERO, 0xba),
LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
RET(REG_ONE),
```

```
$ cargo run --bin fuel-gql-cli -- transaction submit \
"{\"Script\":{\"byte_price\":0,\"gas_price\":0,\"gas_limit\":1000000,\"maturity\":0,\"script\":[80,64,0,202,80,68,0,186,51,65,16,0,36,4,0,0],\"script_data\":[],\"inputs\":[],\"outputs\":[],\"witnesses\":[],\"receipts_root\":\"0x6114142d12e0f58cfb8c72c270cd0535944fb1ba763dce83c17e882c482224a2\"}}"
```
