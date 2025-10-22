# Fuel Client

[![build](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml/badge.svg)](https://github.com/FuelLabs/fuel-core/actions/workflows/ci.yml)
[![deepwiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/FuelLabs/fuel-core)
[![crates.io](https://img.shields.io/crates/v/fuel-core?label=latest)](https://crates.io/crates/fuel-core)
[![docs](https://docs.rs/fuel-core/badge.svg)](https://docs.rs/fuel-core/)
[![discord](https://img.shields.io/badge/chat%20on-discord-orange?&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/xfpK4Pe)
[![twitter](https://img.shields.io/twitter/follow/fuel_network)](https://x.com/fuel_network)

Fuel client implementation.

## Versions currently used in networks

| Network  | Version |
|----------|---------|
| Fuel Ignition | 0.46.0 |
| Testnet | 0.46.0 |
| Devnet | 0.47.1 |

## Contributing

If you are interested in contributing to Fuel, see our [CONTRIBUTING.md](CONTRIBUTING.md) guidelines for coding standards and review process.

Before pushing any changes or creating pull request please run `source ci_checks.sh`.

## Building

If you plan to use already pre-compiled binaries you can directly go to [Running a Ignition node](#running-a-ignition-node).

### System Requirements

There are several system requirements including clang.

#### MacOS

```bash
brew update
brew install cmake
```

#### Debian

```bash
apt update
apt install -y cmake pkg-config build-essential git clang libclang-dev
```

#### Arch

```bash
pacman -Syu --needed --noconfirm cmake gcc pkgconf git clang
```

### Rust setup

You'll need `wasm32-unknown-unknown` target installed.

```bash
rustup target add wasm32-unknown-unknown
```


## Running a Ignition node

If you want to participate in the Ignition network with your own node you can launch it following these simple commands.

### From pre-compiled binaries

Follow : https://docs.fuel.network/docs/node-operator/fuel-ignition/mainnet-node/

### From source

Clone the `fuel-core` repository : 
```
git clone https://github.com/FuelLabs/fuel-core.git
```

Go to the latest release tag for ignition on the `fuel-core` repository :
```
git checkout v0.45.1
```

Build your node binary:
```bash
make build
```

To run the node follow : https://docs.fuel.network/docs/node-operator/fuel-ignition/mainnet-node/

## Running a Local network from source

Clone the `fuel-core` repository : 
```
git clone https://github.com/FuelLabs/fuel-core.git
```

Build your node binary:
```bash
make build
```

To run the node follow : https://docs.fuel.network/docs/node-operator/fuel-ignition/local-node/

## Setup to make contributions

### Compiling

We recommend using `xtask` to build fuel-core for development:

```sh
cargo xtask build
```

This will run `cargo build` as well as any other custom build processes we have such as re-generating a GraphQL schema for the client.

### Testing

The [ci_checks.sh](ci_checks.sh) script file can be used to run all CI checks,
including the running of tests.

```shell
source ci_checks.sh
```

The script requires pre-installed tools. For more information run:

```shell
cat ci_checks.sh
```

## Running

The service can be launched by executing `fuel-core run`. The list of options for running can be accessed via the `help` option:

```console
$ ./target/debug/fuel-core run --help

USAGE:
    fuel-core run [OPTIONS]

OPTIONS:
        --snapshot <SNAPSHOT>
          Snapshot from which to do (re)genesis. Defaults to local testnet configuration

          [env: SNAPSHOT=]
        ...
```

For many development purposes it is useful to have a state that won't persist and the `db-type` option can be set to `in-memory` as in the following example.

### Example

```console
$ ./target/debug/fuel-core run --db-type in-memory
2023-06-13T12:45:22.860536Z  INFO fuel_core::cli::run: 230: Block production mode: Instant
2023-06-13T12:38:47.059783Z  INFO fuel_core::cli::run: 310: Fuel Core version v0.18.1
2023-06-13T12:38:47.078969Z  INFO new{name=fuel-core}:_commit_result{block_id=b1807ca9f2eec7e459b866ecf69b68679fc6b205a9a85c16bd4943d1bfc6fb2a height=0 tx_status=[]}: fuel_core_importer::importer: 231: Committed block
2023-06-13T12:38:47.097777Z  INFO new{name=fuel-core}: fuel_core::graphql_api::service: 208: Binding GraphQL provider to 127.0.0.1:4000
```

To disable block production on your local node, set `--poa-instant=false`

### Example

```console
$ ./target/debug/fuel-core run --poa-instant=false
2023-06-13T12:44:12.857763Z  INFO fuel_core::cli::run: 232: Block production disabled
```

### Troubleshooting

#### Publishing

We use [`publish-crates`](https://github.com/katyo/publish-crates) action for automatic publishing of all crates.

If you have problems with publishing, you can troubleshoot it locally with [`act`](https://github.com/nektos/act).

```shell
act release -s GITHUB_TOKEN=<YOUR_GITHUB_TOKEN> -j publish-crates-check --container-architecture linux/amd64 --reuse
```

It requires GitHubToken to do request to the GitHub. You can create it 
with [this](https://docs.github.com/en/enterprise-server@3.4/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token) instruction.

#### Outdated database

If you encounter an error such as

```console
thread 'main' panicked at 'unable to open database: DatabaseError(Error { message: "Invalid argument: Column families not opened: column-11, column-10, column-9, column-8, column-7, column-6, column-5, column-4, column-3, column-2, column-1, column-0" })', fuel-core/src/main.rs:23:66
```

Clear your local database using: `rm -rf ~/.fuel/db`

#### File descriptor limits

On some macOS versions the default file descriptor limit is quite low, which can lead to IO errors with messages like `Too many open files` or even `fatal runtime error: Rust cannot catch foreign exceptions` when RocksDB encounters these issues. Use the following command to increase the open file limit. Note that this only affects the current shell session, so consider adding it to `~/.zshrc`.

```bash
ulimit -n 10240
```

#### Log level

The service relies on the environment variable `RUST_LOG`. For more information, check the [EnvFilter examples](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#examples) crate.

Human logging can be disabled with the environment variable `HUMAN_LOGGING=false`

## Debugging

See the guide on [debugging](docs/developers/debugging.md) for an overview on running a debug build of a local node.

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

### Transaction executor

The transaction executor currently performs instant block production. Changes are persisted to RocksDB by default.

-   Service endpoint: `/v1/graphql`
-   Schema (available after building): `crates/client/assets/schema.sdl`

The service expects a mutation defined as `submit` that receives a [Transaction](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-tx) in hex encoded binary format, as [specified here](https://github.com/FuelLabs/fuel-specs/blob/master/src/tx-format/transaction.md).

### cURL example

This example will execute a script that represents the following sequence of [ASM](https://github.com/FuelLabs/fuel-vm/tree/master/fuel-asm):

```rs
ADDI(0x10, RegId::ZERO, 0xca),
ADDI(0x11, RegId::ZERO, 0xba),
LOG(0x10, 0x11, RegId::ZERO, RegId::ZERO),
RET(RegId::ONE),
```

```console
$ cargo run --bin fuel-core-client -- transaction submit \
"{\"Script\":{
    \"body\":{
      \"script_gas_limit\":1000000,
      \"receipts_root\":\"0000000000000000000000000000000000000000000000000000000000000000\",
      \"script\":[80,64,0,202,80,68,0,186,51,65,16,0,36,4,0,0],
      \"script_data\":[]
    },
  \"policies\":{
      \"bits\":\"Maturity | MaxFee\",
      \"values\":[0,0,0,0]},
  \"inputs\":[{
    \"CoinSigned\":{
      \"utxo_id\":{
        \"tx_id\":\"c49d65de61cf04588a764b557d25cc6c6b4bc0d7429227e2a21e61c213b3a3e2\",
        \"output_index\":33298
      },
      \"owner\":\"f1e92c42b90934aa6372e30bc568a326f6e66a1a0288595e6e3fbd392a4f3e6e\",
      \"amount\":4294967295,
      \"asset_id\":\"0000000000000000000000000000000000000000000000000000000000000000\",
      \"tx_pointer\":{
        \"block_height\":0,
        \"tx_index\":0
        },
      \"witness_index\":0,
      \"predicate_gas_used\":null,
      \"predicate\":null,
      \"predicate_data\":null}}],
  \"outputs\":[],
  \"witnesses\":[{
    \"data\":[167,184,58,243,113,131,73,255,233,187,213,245,147,97,92,200,55,162,35,88,241,0,222,151,44,66,30,244,186,138,146,161,73,250,79,15,67,105,225,4,79,142,222,72,74,1,221,173,88,143,201,96,229,4,170,19,75,126,67,159,133,151,149,51]}
  ]}}"
```

You may meet the error `Transaction is not inserted. UTXO does not exist` due to the UTXO validation. The UTXO validation can be turned off by adding the `--debug` flag.

```console
$ ./target/debug/fuel-core run --db-type in-memory --debug
```
