# Graph Node

[![Build Status](https://github.com/graphprotocol/graph-node/actions/workflows/ci.yml/badge.svg)](https://github.com/graphprotocol/graph-node/actions/workflows/ci.yml?query=branch%3Amaster)
[![Getting Started Docs](https://img.shields.io/badge/docs-getting--started-brightgreen.svg)](docs/getting-started.md)

[The Graph](https://thegraph.com/) is a protocol for building decentralized applications (dApps) quickly on Ethereum and IPFS using GraphQL.

Graph Node is an open source Rust implementation that event sources the Ethereum blockchain to deterministically update a data store that can be queried via the GraphQL endpoint.

For detailed instructions and more context, check out the [Getting Started Guide](docs/getting-started.md).

## Quick Start

### Prerequisites

To build and run this project you need to have the following installed on your system:

- Rust (latest stable) – [How to install Rust](https://www.rust-lang.org/en-US/install.html)
  - Note that `rustfmt`, which is part of the default Rust installation, is a build-time requirement.
- PostgreSQL – [PostgreSQL Downloads](https://www.postgresql.org/download/)
- IPFS – [Installing IPFS](https://docs.ipfs.io/install/)
- Profobuf Compiler - [Installing Protobuf](https://grpc.io/docs/protoc-installation/)

For Ethereum network data, you can either run your own Ethereum node or use an Ethereum node provider of your choice.

**Minimum Hardware Requirements:**

- To build graph-node with `cargo`, 8GB RAM are required.

### Docker

The easiest way to run a Graph Node is to use the official Docker compose setup. This will start a Postgres database, IPFS node, and Graph Node.
[Follow the instructions here](./docker/README.md).

### Running a Local Graph Node

This is a quick example to show a working Graph Node. It is a [subgraph for Gravatars](https://github.com/graphprotocol/example-subgraph).

1. Install IPFS and run `ipfs init` followed by `ipfs daemon`.
2. Install PostgreSQL and run `initdb -D .postgres -E UTF8 --locale=C` followed by `pg_ctl -D .postgres -l logfile start` and `createdb graph-node`.
3. If using Ubuntu, you may need to install additional packages:
   - `sudo apt-get install -y clang libpq-dev libssl-dev pkg-config`
4. In the terminal, clone https://github.com/graphprotocol/example-subgraph, and install dependencies and generate types for contract ABIs:

```
yarn
yarn codegen
```

5. In the terminal, clone https://github.com/graphprotocol/graph-node, and run `cargo build`.

Once you have all the dependencies set up, you can run the following:

```
cargo run -p graph-node --release -- \
  --postgres-url postgresql://USERNAME[:PASSWORD]@localhost:5432/graph-node \
  --ethereum-rpc NETWORK_NAME:[CAPABILITIES]:URL \
  --ipfs 127.0.0.1:5001
```

Try your OS username as `USERNAME` and `PASSWORD`. For details on setting
the connection string, check the [Postgres
documentation](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING).
`graph-node` uses a few Postgres extensions. If the Postgres user with which
you run `graph-node` is a superuser, `graph-node` will enable these
extensions when it initializes the database. If the Postgres user is not a
superuser, you will need to create the extensions manually since only
superusers are allowed to do that. To create them you need to connect as a
superuser, which in many installations is the `postgres` user:

```bash
    psql -q -X -U <SUPERUSER> graph-node <<EOF
create extension pg_trgm;
create extension pg_stat_statements;
create extension btree_gist;
create extension postgres_fdw;
grant usage on foreign data wrapper postgres_fdw to <USERNAME>;
EOF

```

This will also spin up a GraphiQL interface at `http://127.0.0.1:8000/`.

6.  With this Gravatar example, to get the subgraph working locally run:

```
yarn create-local
```

Then you can deploy the subgraph:

```
yarn deploy-local
```

This will build and deploy the subgraph to the Graph Node. It should start indexing the subgraph immediately.

### Command-Line Interface

```
USAGE:
    graph-node [FLAGS] [OPTIONS] --ethereum-ipc <NETWORK_NAME:FILE> --ethereum-rpc <NETWORK_NAME:URL> --ethereum-ws <NETWORK_NAME:URL> --ipfs <HOST:PORT> --postgres-url <URL>

FLAGS:
        --debug      Enable debug logging
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --admin-port <PORT>                           Port for the JSON-RPC admin server [default: 8020]
        --elasticsearch-password <PASSWORD>
            Password to use for Elasticsearch logging [env: ELASTICSEARCH_PASSWORD]

        --elasticsearch-url <URL>
            Elasticsearch service to write subgraph logs to [env: ELASTICSEARCH_URL=]

        --elasticsearch-user <USER>                   User to use for Elasticsearch logging [env: ELASTICSEARCH_USER=]
        --ethereum-ipc <NETWORK_NAME:[CAPABILITIES]:FILE>
            Ethereum network name (e.g. 'mainnet'), optional comma-separated capabilities (eg full,archive), and an Ethereum IPC pipe, separated by a ':'

        --ethereum-polling-interval <MILLISECONDS>
            How often to poll the Ethereum node for new blocks [env: ETHEREUM_POLLING_INTERVAL=]  [default: 500]

        --ethereum-rpc <NETWORK_NAME:[CAPABILITIES]:URL>
            Ethereum network name (e.g. 'mainnet'), optional comma-separated capabilities (eg 'full,archive'), and an Ethereum RPC URL, separated by a ':'

        --ethereum-ws <NETWORK_NAME:[CAPABILITIES]:URL>
            Ethereum network name (e.g. 'mainnet'), optional comma-separated capabilities (eg `full,archive), and an Ethereum WebSocket URL, separated by a ':'

        --node-id <NODE_ID>
            A unique identifier for this node instance. Should have the same value between consecutive node restarts [default: default]

        --http-port <PORT>                            Port for the GraphQL HTTP server [default: 8000]
        --ipfs <HOST:PORT>                            HTTP address of an IPFS node
        --postgres-url <URL>                          Location of the Postgres database used for storing entities
        --subgraph <[NAME:]IPFS_HASH>                 Name and IPFS hash of the subgraph manifest
        --ws-port <PORT>                              Port for the GraphQL WebSocket server [default: 8001]
```

### Advanced Configuration

The command line arguments generally are all that is needed to run a
`graph-node` instance. For advanced uses, various aspects of `graph-node`
can further be configured through [environment
variables](https://github.com/graphprotocol/graph-node/blob/master/docs/environment-variables.md). Very
large `graph-node` instances can also split the work of querying and
indexing across [multiple databases](./docs/config.md).

## Project Layout

- `node` — A local Graph Node.
- `graph` — A library providing traits for system components and types for
  common data.
- `core` — A library providing implementations for core components, used by all
  nodes.
- `chain/ethereum` — A library with components for obtaining data from
  Ethereum.
- `graphql` — A GraphQL implementation with API schema generation,
  introspection, and more.
- `mock` — A library providing mock implementations for all system components.
- `runtime/wasm` — A library for running WASM data-extraction scripts.
- `server/http` — A library providing a GraphQL server over HTTP.
- `store/postgres` — A Postgres store with a GraphQL-friendly interface
  and audit logs.

## Roadmap

🔨 = In Progress

🛠 = Feature complete. Additional testing required.

✅ = Feature complete


| Feature |  Status |
| ------- |  :------: |
| **Ethereum** |    |
| Indexing smart contract events | ✅ |
| Handle chain reorganizations | ✅ |
| **Mappings** |    |
| WASM-based mappings| ✅ |
| TypeScript-to-WASM toolchain | ✅ |
| Autogenerated TypeScript types | ✅ |
| **GraphQL** |     |
| Query entities by ID | ✅ |
| Query entity collections | ✅ |
| Pagination | ✅ |
| Filtering | ✅ |
| Block-based Filtering | ✅ |
| Entity relationships | ✅ |
| Subscriptions | ✅ |


## Contributing

Please check [CONTRIBUTING.md](CONTRIBUTING.md) for development flow and conventions we use.
Here's [a list of good first issues](https://github.com/graphprotocol/graph-node/labels/good%20first%20issue).

## License

Copyright &copy; 2018-2019 Graph Protocol, Inc. and contributors.

The Graph is dual-licensed under the [MIT license](LICENSE-MIT) and the [Apache License, Version 2.0](LICENSE-APACHE).

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either expressed or implied. See the License for the specific language governing permissions and limitations under the License.
