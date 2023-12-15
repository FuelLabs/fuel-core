# Subgraph Manifest
##### v.0.0.4

## 1.1 Overview
The subgraph manifest specifies all the information required to index and query a specific subgraph. This is the entry point to your subgraph.

The subgraph manifest, and all the files linked from it, is what is deployed to IPFS and hashed to produce a subgraph ID that can be referenced and used to retrieve your subgraph in The Graph.

## 1.2 Format
Any data format that has a well-defined 1:1 mapping with the [IPLD Canonical Format](https://github.com/ipld/specs/) may be used to define a subgraph manifest. This includes YAML and JSON. Examples in this document are in YAML.

## 1.3 Top-Level API

| Field  | Type | Description   |
| --- | --- | --- |
| **specVersion** | *String*   | A Semver version indicating which version of this API is being used.|
| **schema**   | [*Schema*](#14-schema) | The GraphQL schema of this subgraph.|
| **description**   | *String* | An optional description of the subgraph's purpose. |
| **repository**   | *String* | An optional link to where the subgraph lives. |
| **graft** | optional [*Graft Base*](#18-graft-base) | An optional base to graft onto. |
| **dataSources**| [*Data Source Spec*](#15-data-source)| Each data source spec defines the data that will be ingested as well as the transformation logic to derive the state of the subgraph's entities based on the source data.|
| **templates** | [*Data Source Templates Spec*](#17-data-source-templates) | Each data source template defines a data source that can be created dynamically from the mappings. |
| **features** | optional [*[String]*](#19-features) | A list of feature names used by the subgraph. |

## 1.4 Schema

| Field | Type | Description |
| --- | --- | --- |
| **file**| [*Path*](#16-path) | The path of the GraphQL IDL file, either local or on IPFS. |

## 1.5 Data Source

| Field | Type | Description |
| --- | --- | --- |
| **kind** | *String | The type of data source. Possible values: *ethereum/contract*.|
| **name** | *String* | The name of the source data. Will be used to generate APIs in the mapping and also for self-documentation purposes. |
| **network** | *String* | For blockchains, this describes which network the subgraph targets. For Ethereum, this can be any of "mainnet", "rinkeby", "kovan", "ropsten", "goerli", "poa-core", "poa-sokol", "xdai", "matic", "mumbai", "fantom", "bsc" or "clover". Developers could look for an up to date list in the graph-cli [*code*](https://github.com/graphprotocol/graph-tooling/blob/main/packages/cli/src/protocols/index.ts#L76-L117).|
| **source** | [*EthereumContractSource*](#151-ethereumcontractsource) | The source data on a blockchain such as Ethereum. |
| **mapping** | [*Mapping*](#152-mapping) | The transformation logic applied to the data prior to being indexed. |

### 1.5.1 EthereumContractSource

| Field | Type | Description |
| --- | --- | --- |
| **address** | *String* | The address of the source data in its respective blockchain. |
| **abi** | *String* | The name of the ABI for this Ethereum contract. See `abis` in the `mapping` manifest. |
| **startBlock** | optional *BigInt* | The block to start indexing this data source from. |


### 1.5.2 Mapping
The `mapping` field may be one of the following supported mapping manifests:
 - [Ethereum Mapping](#1521-ethereum-mapping)

#### 1.5.2.1 Ethereum Mapping

| Field | Type | Description |
| --- | --- | --- |
| **kind** | *String* | Must be "ethereum/events" for Ethereum Events Mapping. |
| **apiVersion** | *String* | Semver string of the version of the Mappings API that will be used by the mapping script. |
| **language** | *String* | The language of the runtime for the Mapping API. Possible values: *wasm/assemblyscript*. |
| **entities** | *[String]* | A list of entities that will be ingested as part of this mapping. Must correspond to names of entities in the GraphQL IDL. |
| **abis** | *ABI* | ABIs for the contract classes that should be generated in the Mapping ABI. Name is also used to reference the ABI elsewhere in the manifest. |
| **eventHandlers** | optional *EventHandler* | Handlers for specific events, which will be defined in the mapping script. |
| **callHandlers** | optional *CallHandler* | A list of functions that will trigger a  handler and the name of the corresponding handlers in the mapping. |
| **blockHandlers** | optional *BlockHandler* | Defines block filters and handlers to process matching blocks. |
| **file** | [*Path*](#16-path) | The path of the mapping script. |

> **Note:** Each mapping is required to supply one or more handler type, available types: `EventHandler`, `CallHandler`, or `BlockHandler`.

#### 1.5.2.2 EventHandler

| Field | Type | Description |
| --- | --- | --- |
| **event** | *String* | An identifier for an event that will be handled in the mapping script. For Ethereum contracts, this must be the full event signature to distinguish from events that may share the same name. No alias types can be used. For example, uint will not work, uint256 must be used.|
| **handler** | *String* | The name of an exported function in the mapping script that should handle the specified event. |
| **topic0** | optional *String* | A `0x` prefixed hex string. If provided, events whose topic0 is equal to this value will be processed by the given handler. When topic0 is provided, _only_ the topic0 value will be matched, and not the hash of the event signature. This is useful for processing anonymous events in Solidity, which can have their topic0 set to anything.  By default, topic0 is equal to the hash of the event signature. |

#### 1.5.2.3 CallHandler

| Field | Type | Description |
| --- | --- | --- |
| **function** | *String* | An identifier for a function that will be handled in the mapping script. For Ethereum contracts, this is the normalized function signature to filter calls by. |
| **handler** | *String* | The name of an exported function in the mapping script that should handle the specified event. |

#### 1.5.2.4 BlockHandler

| Field | Type | Description |
| --- | --- | --- |
| **handler** | *String* | The name of an exported function in the mapping script that should handle the specified event. |
| **filter** | optional *BlockHandlerFilter* | Definition of the filter to apply. If none is supplied, the handler will be called on every block. |

#### 1.5.2.4.1 BlockHandlerFilter

| Field | Type | Description |
| --- | --- | --- |
| **kind** | *String* | The selected block handler filter. Only option for now: `call`: This will only run the handler if the block contains at least one call to the data source contract. |

## 1.6 Path
A path has one field `path`, which either refers to a path of a file on the local dev machine or an [IPLD link](https://github.com/ipld/specs/).

When using the Graph-CLI, local paths may be used during development, and then, the tool will take care of deploying linked files to IPFS and replacing the local paths with IPLD links at deploy time.

| Field | Type | Description |
| --- | --- | --- |
| **path** | *String or [IPLD Link](https://github.com/ipld/specs/)* | A path to a local file or IPLD link. |

## 1.7 Data Source Templates
A data source template has all of the fields of a normal data source, except it does not include a contract address under `source`. The address is a parameter that can later be provided when creating a dynamic data source from the template.
```yml
# ...
templates:
  - name: Exchange
    kind: ethereum/contract
    network: mainnet
    source:
      abi: Exchange
    mapping:
      kind: ethereum/events
      apiVersion: 0.0.1
      language: wasm/assemblyscript
      file: ./src/mappings/exchange.ts
      entities:
        - Exchange
      abis:
        - name: Exchange
          file: ./abis/exchange.json
      eventHandlers:
        - event: TokenPurchase(address,uint256,uint256)
          handler: handleTokenPurchase
```

## 1.8 Graft Base
A subgraph can be _grafted_ on top of another subgraph, meaning that, rather than starting to index the subgraph from the genesis block, the subgraph is initialized with a copy of the given base subgraph, and indexing resumes from the given block.

| Field | Type | Description |
| --- | --- | --- |
| **base** | *String* | The subgraph ID of the base subgraph |
| **block** | *BigInt* | The block number up to which to use data from the base subgraph |

## 1.9 Features

Starting from `specVersion` `0.0.4`, a subgraph must declare all _feature_ names it uses to be
considered valid.

A Graph Node instance will **reject** a subgraph deployment if:
- the `specVersion` is equal to or higher than `0.0.4` **AND**
- it hasn't explicitly declared a feature it uses.

No validation errors will happen if a feature is declared but not used.

These are the currently available features and their names:

| Feature                    | Name                      |
| ---                        | ---                       |
| Non-fatal errors           | `nonFatalErrors`          |
| Full-text Search           | `fullTextSearch`          |
| Grafting                   | `grafting`                |
| IPFS on Ethereum Contracts | `ipfsOnEthereumContracts` |
