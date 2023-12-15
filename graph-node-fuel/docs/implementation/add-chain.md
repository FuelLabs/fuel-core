# Adding support for a new chain

## Context

`graph-node` started as a project that could only index EVM compatible chains, eg: `ethereum`, `xdai`, etc.

It was known from the start that with growth we would like `graph-node` to be able to index other chains like `NEAR`, `Solana`, `Cosmos`, list goes on...

However to do it, several refactors were necessary, because the code had a great amount of assumptions based of how Ethereum works.

At first there was a [RFC](https://github.com/graphprotocol/rfcs/blob/10aaae30fdf82f0dd2ccdf4bbecf7ec6bbfb703b/rfcs/0005-multi-blockchain-support.md) for a design overview, then actual PRs such as:

- https://github.com/graphprotocol/graph-node/pull/2272
- https://github.com/graphprotocol/graph-node/pull/2292
- https://github.com/graphprotocol/graph-node/pull/2399
- https://github.com/graphprotocol/graph-node/pull/2411
- https://github.com/graphprotocol/graph-node/pull/2453
- https://github.com/graphprotocol/graph-node/pull/2463
- https://github.com/graphprotocol/graph-node/pull/2755

All new chains, besides the EVM compatible ones, are integrated using [StreamingFast](https://www.streamingfast.io/)'s [Firehose](https://firehose.streamingfast.io/). The integration consists of chain specific `protobuf` files with the type definitions.

## How to do it?

The `graph-node` repository contains multiple Rust crates in it, this section will be divided in each of them that needs to be modified/created.

> It's important to remember that this document is static and may not be up to date with the current implementation. Be aware too that it won't contain all that's needed, it's mostly listing the main areas that need change.

### chain

You'll need to create a new crate in the [chain folder](https://github.com/graphprotocol/graph-node/tree/1cd7936f9143f317feb51be1fc199122761fcbb1/chain) with an appropriate name and the same `version` as the rest of the other ones.

> Note: you'll probably have to add something like `graph-chain-{{CHAIN_NAME}} = { path = "../chain/{{CHAIN_NAME}}" }` to the `[dependencies]` section of a few other `Cargo.toml` files

It's here that you add the `protobuf` definitions with the specific types for the chain you're integrating with. Examples:

- [Ethereum](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/chain/ethereum/proto/codec.proto)
- [NEAR](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/chain/near/proto/codec.proto)
- [Cosmos](https://github.com/graphprotocol/graph-node/blob/caa54c1039d3c282ac31bb0e96cb277dbf82f793/chain/cosmos/proto/type.proto)

To compile those we use a crate called `tonic`, it will require a [`build.rs` file](https://doc.rust-lang.org/cargo/reference/build-scripts.html) like the one in the other folders/chains, eg:

```rust
fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .out_dir("src/protobuf")
        .compile(&["proto/codec.proto"], &["proto"])
        .expect("Failed to compile Firehose CoolChain proto(s)");
}
```

You'll also need a `src/codec.rs` to extract the data from the generated Rust code, much like [this one](https://github.com/graphprotocol/graph-node/blob/caa54c1039d3c282ac31bb0e96cb277dbf82f793/chain/cosmos/src/codec.rs).

Besides this source file, there should also be a `TriggerFilter`, `NodeCapabilities` and `RuntimeAdapter`, here are a few empty examples:

`src/adapter.rs`
```rust
use crate::capabilities::NodeCapabilities;
use crate::{data_source::DataSource, Chain};
use graph::blockchain as bc;
use graph::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, _data_sources: impl Iterator<Item = &'a DataSource> + Clone) {}

    fn node_capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {}
    }

    fn extend_with_template(
        &mut self,
        _data_source: impl Iterator<Item = <Chain as bc::Blockchain>::DataSourceTemplate>,
    ) {
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        vec![]
    }
}
```

`src/capabilities.rs`
```rust
use std::cmp::PartialOrd;
use std::fmt;
use std::str::FromStr;

use anyhow::Error;
use graph::impl_slog_value;

use crate::DataSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd)]
pub struct NodeCapabilities {}

impl FromStr for NodeCapabilities {
    type Err = Error;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Ok(NodeCapabilities {})
    }
}

impl fmt::Display for NodeCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("{{CHAIN_NAME}}")
    }
}

impl_slog_value!(NodeCapabilities, "{}");

impl graph::blockchain::NodeCapabilities<crate::Chain> for NodeCapabilities {
    fn from_data_sources(_data_sources: &[DataSource]) -> Self {
        NodeCapabilities {}
    }
}
```

`src/runtime/runtime_adapter.rs`
```rust
use crate::{Chain, DataSource};
use anyhow::Result;
use blockchain::HostFn;
use graph::blockchain;

pub struct RuntimeAdapter {}

impl blockchain::RuntimeAdapter<Chain> for RuntimeAdapter {
    fn host_fns(&self, _ds: &DataSource) -> Result<Vec<HostFn>> {
        Ok(vec![])
    }
}
```

The chain specific type definitions should also be available for the `runtime`. Since it comes mostly from the `protobuf` files, there's a [generation tool](https://github.com/streamingfast/graph-as-to-rust) made by StreamingFast that you can use to create the `src/runtime/generated.rs`.

You'll also have to implement `ToAscObj` for those types, that usually is made in a `src/runtime/abi.rs` file.

Another thing that will be needed is the `DataSource` types for the [subgraph manifest](https://thegraph.com/docs/en/developer/create-subgraph-hosted/#the-subgraph-manifest).

`src/data_source.rs`
```rust
#[derive(Clone, Debug)]
pub struct DataSource {
    // example fields:
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub source: Source,
    pub mapping: Mapping,
    pub context: Arc<Option<DataSourceContext>>,
    pub creation_block: Option<BlockNumber>,
    /*...*/
}

impl blockchain::DataSource<Chain> for DataSource { /*...*/ }

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub source: Source,
    pub mapping: UnresolvedMapping,
    pub context: Option<DataSourceContext>,
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource { /*...*/ }

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct BaseDataSourceTemplate<M> {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub mapping: M,
}

pub type UnresolvedDataSourceTemplate = BaseDataSourceTemplate<UnresolvedMapping>;
pub type DataSourceTemplate = BaseDataSourceTemplate<Mapping>;

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for UnresolvedDataSourceTemplate { /*...*/ }

impl blockchain::DataSourceTemplate<Chain> for DataSourceTemplate { /*...*/ }
```

And at last, the type that will glue them all, the `Chain` itself.

`src/chain.rs`
```rust
pub struct Chain { /*...*/ }

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::CoolChain;

    type Block = codec::...;

    type DataSource = DataSource;

    // ...

    type TriggerFilter = TriggerFilter;

    type NodeCapabilities = NodeCapabilities;

    type RuntimeAdapter = RuntimeAdapter;
}

pub struct TriggersAdapter { /*...*/ }

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter { /*...*/ }

pub struct FirehoseMapper {
    endpoint: Arc<FirehoseEndpoint>,
}

#[async_trait]
impl FirehoseMapperTrait<Chain> for FirehoseMapper { /*...*/ }
```

### node

The `src/main.rs` file should be able to handle the connection to the new chain via Firehose for the startup, similar to [this](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/node/src/main.rs#L255).

### graph

Two changes are required here:

1. [BlockchainKind](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/graph/src/blockchain/mod.rs#L309) needs to have a new variant for the chain you're integrating with.
2. And the [IndexForAscTypeId](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/graph/src/runtime/mod.rs#L147) should have the new variants for the chain specific types of the `runtime`.

### server

You'll just have to handle the new `BlockchainKind` in the [index-node/src/resolver.rs](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/server/index-node/src/resolver.rs#L361).

### core

Just like in the `server` crate, you'll just have to handle the new `BlockchainKind` in the [SubgraphInstanceManager](https://github.com/graphprotocol/graph-node/blob/1cd7936f9143f317feb51be1fc199122761fcbb1/core/src/subgraph/instance_manager.rs#L41).

## Example Integrations (PRs)

- NEAR by StreamingFast
  - https://github.com/graphprotocol/graph-node/pull/2820
- Cosmos by Figment
  - https://github.com/graphprotocol/graph-node/pull/3212
  - https://github.com/graphprotocol/graph-node/pull/3543
- Solana by StreamingFast
  - https://github.com/graphprotocol/graph-node/pull/3210

## What else?

Besides making `graph-node` support the new chain, [graph-cli](https://github.com/graphprotocol/graph-tooling/tree/main/packages/cli) and [graph-ts](https://github.com/graphprotocol/graph-tooling/tree/main/packages/ts) should also include the new types and enable the new functionality so that subgraph developers can use it.

For now this document doesn't include how to do that integration, here are a few PRs that might help you with that:

- NEAR
  - `graph-cli`
    - https://github.com/graphprotocol/graph-tooling/pull/760
    - https://github.com/graphprotocol/graph-tooling/pull/783
  - `graph-ts`
    - https://github.com/graphprotocol/graph-ts/pull/210
    - https://github.com/graphprotocol/graph-ts/pull/217
- Cosmos
  - `graph-cli`
    - https://github.com/graphprotocol/graph-tooling/pull/827
    - https://github.com/graphprotocol/graph-tooling/pull/851
    - https://github.com/graphprotocol/graph-toolingpull/888
  - `graph-ts`
    - https://github.com/graphprotocol/graph-ts/pull/250
    - https://github.com/graphprotocol/graph-ts/pull/273

Also this document doesn't include the multi-blockchain part required for The Graph Network, which at this current moment is in progress, for now the network only supports Ethereum `mainnet`.
