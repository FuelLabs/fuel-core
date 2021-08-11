# Fuel Client

Fuel client implementation.

## Testing

The test suite follows the Rust cargo standards. The GraphQL service will be instantiated by Actix testing runtime and will emulate a server/client structure.

To run the suite:
`$ cargo test`

## Building

For optimal performance, we recommend using native builds. The generated binary will be optimized for your CPU and may contain specific instructions supported only in your hardware.

To build, run:
`$ RUSTFLAGS="-C target-cpu=native" cargo build --release`

The generated binary will be located in `./target/release/fuel-core`

## Running

The service can listen to an arbitrary socket, as specified in the help command:

```
$ ./target/release/fuel-core --help
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
$ ./target/release/fuel-core --ip 127.0.0.1 --port 4000
Jul 12 23:28:47.238  INFO fuel_core: Binding GraphQL provider to 127.0.0.1:4000
Jul 12 23:28:47.238  INFO actix_server::builder: Starting 8 workers
Jul 12 23:28:47.239  INFO actix_server::builder: Starting "actix-web-service-127.0.0.1:4000" service on 127.0.0.1:4000
```

#### Log level

The service relies on the environment variable `RUST_LOG`. For more information, check the [env_logger](https://docs.rs/env_logger) crate.

## GraphQL service

The client functionality is available through service endpoints that expect GraphQL queries.

#### Transaction executor

The transaction executor will have in-memory storage with its lifetime bound to the GraphQL process - meaning that if the service restarts, then the storage state is reset.

* Service endpoint: `/tx`
* Schema (available after building): `tx-client/assets/tx.sdl`

The service expects a mutation defined as `run` that receives a [Transaction](https://github.com/FuelLabs/fuel-tx) in JSON format, as specified in `docs/tx-schema.json`.

##### cURL example

This example will execute a script that represents the following sequence of [ASM](https://github.com/FuelLabs/fuel-asm):

```
ADDI(0x10, REG_ZERO, 0xca),
ADDI(0x11, REG_ZERO, 0xba),
LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
RET(REG_ONE),
```

```
$ curl -X POST \
-H "Content-Type: application/json" \
-d '
{
   "query":"mutation Mutation($tx: String!) { run(tx: $tx)}",
   "variables":{
      "tx":"{\"Script\":{\"gas_price\":0,\"gas_limit\":1000000,\"maturity\":0,\"script\":[17,64,0,202,17,68,0,186,89,65,16,0,52,4,0,0],\"script_data\":[],\"inputs\":[],\"outputs\":[],\"witnesses\":[]}}"
   }
}' \
http://127.0.0.1:4000/tx
```
