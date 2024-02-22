# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

Description of the upcoming release here.

### Added

- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): Added a new `Merklized` blueprint that maintains the binary Merkle tree over the storage data. It supports only the insertion of the objects without removing them.

### Changed

- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): The logic related to the `FuelBlockIdsToHeights` is moved to the off-chain worker.
- [#1663](https://github.com/FuelLabs/fuel-core/pull/1663): Reduce the punishment criteria for mempool gossipping.
- [#1658](https://github.com/FuelLabs/fuel-core/pull/1658): Removed `Receipts` table. Instead, receipts are part of the `TransactionStatuses` table.
- [#1640](https://github.com/FuelLabs/fuel-core/pull/1640): Upgrade to fuel-vm 0.45.0.
- [#1635](https://github.com/FuelLabs/fuel-core/pull/1635): Move updating of the owned messages and coins to off-chain worker.
- [#1650](https://github.com/FuelLabs/fuel-core/pull/1650): Add api endpoint for getting estimates for future gas prices
- [#1649](https://github.com/FuelLabs/fuel-core/pull/1649): Add api endpoint for getting latest gas price
- [#1600](https://github.com/FuelLabs/fuel-core/pull/1640): Upgrade to fuel-vm 0.45.0
- [#1633](https://github.com/FuelLabs/fuel-core/pull/1633): Notify services about importing of the genesis block.
- [#1625](https://github.com/FuelLabs/fuel-core/pull/1625): Making relayer independent from the executor and preparation for the force transaction inclusion.
- [#1613](https://github.com/FuelLabs/fuel-core/pull/1613): Add api endpoint to retrieve a message by its nonce.
- [#1612](https://github.com/FuelLabs/fuel-core/pull/1612): Use `AtomicView` in all services for consistent results.
- [#1597](https://github.com/FuelLabs/fuel-core/pull/1597): Unify namespacing for `libp2p` modules
- [#1591](https://github.com/FuelLabs/fuel-core/pull/1591): Simplify libp2p dependencies and not depend on all sub modules directly.
- [#1590](https://github.com/FuelLabs/fuel-core/pull/1590): Use `AtomicView` in the `TxPool` to read the state of the database during insertion of the transactions.
- [#1587](https://github.com/FuelLabs/fuel-core/pull/1587): Use `BlockHeight` as a primary key for the `FuelsBlock` table.
- [#1585](https://github.com/FuelLabs/fuel-core/pull/1585): Let `NetworkBehaviour` macro generate `FuelBehaviorEvent` in p2p
- [#1579](https://github.com/FuelLabs/fuel-core/pull/1579): The change extracts the off-chain-related logic from the executor and moves it to the GraphQL off-chain worker. It creates two new concepts - Off-chain and On-chain databases where the GraphQL worker has exclusive ownership of the database and may modify it without intersecting with the On-chain database.
- [#1577](https://github.com/FuelLabs/fuel-core/pull/1577): Moved insertion of sealed blocks into the `BlockImporter` instead of the executor.
- [#1574](https://github.com/FuelLabs/fuel-core/pull/1574): Penalizes peers for sending invalid responses or for not replying at all.
- [#1601](https://github.com/FuelLabs/fuel-core/pull/1601): Fix formatting in docs and check that `cargo doc` passes in the CI.
- [#1636](https://github.com/FuelLabs/fuel-core/pull/1636): Add more docs to GraphQL DAP API.

#### Breaking
- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): The GraphQL API uses block height instead of the block id where it is possible. The transaction status contains `block_height` instead of the `block_id`.
- [#1675](https://github.com/FuelLabs/fuel-core/pull/1675): Simplify GQL schema by disabling contract resolvers in most cases, and just return a ContractId scalar instead.
- [#1658](https://github.com/FuelLabs/fuel-core/pull/1658): Receipts are part of the transaction status. 
    Removed `reason` from the `TransactionExecutionResult::Failed`. It can be calculated based on the program state and receipts.
    Also, it is not possible to fetch `receipts` from the `Transaction` directly anymore. Instead, you need to fetch `status` and its receipts.
- [#1646](https://github.com/FuelLabs/fuel-core/pull/1646): Remove redundant receipts from queries.
- [#1639](https://github.com/FuelLabs/fuel-core/pull/1639): Make Merkle metadata, i.e. `SparseMerkleMetadata` and `DenseMerkleMetadata` type version-able enums
- [#1632](https://github.com/FuelLabs/fuel-core/pull/1632): Make `Message` type a version-able enum
- [#1631](https://github.com/FuelLabs/fuel-core/pull/1631): Modify api endpoint to dry run multiple transactions.
- [#1629](https://github.com/FuelLabs/fuel-core/pull/1629): Use a separate database for each data domain. Each database has its own folder where data is stored.
- [#1628](https://github.com/FuelLabs/fuel-core/pull/1628): Make `CompressedCoin` type a version-able enum
- [#1616](https://github.com/FuelLabs/fuel-core/pull/1616): Make `BlockHeader` type a version-able enum
- [#1614](https://github.com/FuelLabs/fuel-core/pull/1614): Use the default consensus key regardless of trigger mode. The change is breaking because it removes the `--dev-keys` argument. If the `debug` flag is set, the default consensus key will be used, regardless of the trigger mode.
- [#1596](https://github.com/FuelLabs/fuel-core/pull/1596): Make `Consensus` type a version-able enum
- [#1593](https://github.com/FuelLabs/fuel-core/pull/1593): Make `Block` type a version-able enum
- [#1576](https://github.com/FuelLabs/fuel-core/pull/1576): The change moves the implementation of the storage traits for required tables from `fuel-core` to `fuel-core-storage` crate. The change also adds a more flexible configuration of the encoding/decoding per the table and allows the implementation of specific behaviors for the table in a much easier way. It unifies the encoding between database, SMTs, and iteration, preventing mismatching bytes representation on the Rust type system level. Plus, it increases the re-usage of the code by applying the same blueprint to other tables.
    
    It is a breaking PR because it changes database encoding/decoding for some tables.
    
    ### StructuredStorage
    
    The change adds a new type `StructuredStorage`. It is a wrapper around the key-value storage that implements the storage traits(`StorageInspect`, `StorageMutate`, `StorageRead`, etc) for the tables with blueprint. This blueprint works in tandem with the `TableWithBlueprint` trait. The table may implement `TableWithBlueprint` specifying the blueprint, as an example:
    
    ```rust
    impl TableWithBlueprint for ContractsRawCode {
        type Blueprint = Plain<Raw, Raw>;
    
        fn column() -> Column {
            Column::ContractsRawCode
        }
    }
    ```
    
    It is a definition of the blueprint for the `ContractsRawCode` table. It has a plain blueprint meaning it simply encodes/decodes bytes and stores/loads them into/from the storage. As a key codec and value codec, it uses a `Raw` encoding/decoding that simplifies writing bytes and loads them back into the memory without applying any serialization or deserialization algorithm.
    
    If the table implements `TableWithBlueprint` and the selected codec satisfies all blueprint requirements, the corresponding storage traits for that table are implemented on the `StructuredStorage` type.
    
    ### Codecs
    
    Each blueprint allows customizing the key and value codecs. It allows the use of different codecs for different tables, taking into account the complexity and weight of the data and providing a way of more optimal implementation.
    
    That property may be very useful to perform migration in a more easier way. Plus, it also can be a `no_std` migration potentially allowing its fraud proving.
    
    An example of migration:
    
    ```rust
    /// Define the table for V1 value encoding/decoding.
    impl TableWithBlueprint for ContractsRawCodeV1 {
        type Blueprint = Plain<Raw, Raw>;
    
        fn column() -> Column {
            Column::ContractsRawCode
        }
    }
    
    /// Define the table for V2 value encoding/decoding.
    /// It uses `Postcard` codec for the value instead of `Raw` codec.
    ///
    /// # Dev-note: The columns is the same.
    impl TableWithBlueprint for ContractsRawCodeV2 {
        type Blueprint = Plain<Raw, Postcard>;
    
        fn column() -> Column {
            Column::ContractsRawCode
        }
    }
    
    fn migration(storage: &mut Database) {
        let mut iter = storage.iter_all::<ContractsRawCodeV1>(None);
        while let Ok((key, value)) = iter.next() {
            // Insert into the same table but with another codec.
            storage.storage::<ContractsRawCodeV2>().insert(key, value);
        }
    }
    ```
    
    ### Structures
    
    The blueprint of the table defines its behavior. As an example, a `Plain` blueprint simply encodes/decodes bytes and stores/loads them into/from the storage. The `SMT` blueprint builds a sparse merkle tree on top of the key-value pairs.
    
    Implementing a blueprint one time, we can apply it to any table satisfying the requirements of this blueprint. It increases the re-usage of the code and minimizes duplication.
    
    It can be useful if we decide to create global roots for all required tables that are used in fraud proving.
    
    ```rust
    impl TableWithBlueprint for SpentMessages {
        type Blueprint = Plain<Raw, Postcard>;
    
        fn column() -> Column {
            Column::SpentMessages
        }
    }
                     |
                     |
                    \|/
    
    impl TableWithBlueprint for SpentMessages {
        type Blueprint =
            Sparse<Raw, Postcard, SpentMessagesMerkleMetadata, SpentMessagesMerkleNodes>;
    
        fn column() -> Column {
            Column::SpentMessages
        }
    }
    ```
    
    ### Side changes
    
    #### `iter_all`
    The `iter_all` functionality now accepts the table instead of `K` and `V` generics. It is done to use the correct codec during deserialization. Also, the table definition provides the column.
    
    #### Duplicated unit tests
    
    The `fuel-core-storage` crate provides macros that generate unit tests. Almost all tables had the same test like `get`, `insert`, `remove`, `exist`. All duplicated tests were moved to macros. The unique one still stays at the same place where it was before.
    
    #### `StorageBatchMutate`
    
    Added a new `StorageBatchMutate` trait that we can move to `fuel-storage` crate later. It allows batch operations on the storage. It may be more performant in some cases.

- [#1573](https://github.com/FuelLabs/fuel-core/pull/1573): Remove nested p2p request/response encoding. Only breaks p2p networking compatibility with older fuel-core versions, but is otherwise fully internal.


## [Version 0.22.1]

### Fixed
- [#1664](https://github.com/FuelLabs/fuel-core/pull/1664): Fixed long database initialization after restart of the node by setting limit to the WAL file.


## [Version 0.22.0]

### Added

- [#1515](https://github.com/FuelLabs/fuel-core/pull/1515): Added support of `--version` command for `fuel-core-keygen` binary.
- [#1504](https://github.com/FuelLabs/fuel-core/pull/1504): A `Success` or `Failure` variant of `TransactionStatus` returned by a query now contains the associated receipts generated by transaction execution.

#### Breaking
- [#1531](https://github.com/FuelLabs/fuel-core/pull/1531): Make `fuel-core-executor` `no_std` compatible. It affects the `fuel-core` crate because it uses the `fuel-core-executor` crate. The change is breaking because of moved types.
- [#1524](https://github.com/FuelLabs/fuel-core/pull/1524): Adds information about connected peers to the GQL API.

### Changed

- [#1517](https://github.com/FuelLabs/fuel-core/pull/1517): Changed default gossip heartbeat interval to 500ms. 
- [#1520](https://github.com/FuelLabs/fuel-core/pull/1520): Extract `executor` into `fuel-core-executor` crate.

### Fixed

#### Breaking
- [#1536](https://github.com/FuelLabs/fuel-core/pull/1536): The change fixes the contracts tables to not touch SMT nodes of foreign contracts. Before, it was possible to invalidate the SMT from another contract. It is a breaking change and requires re-calculating the whole state from the beginning with new SMT roots. 
- [#1542](https://github.com/FuelLabs/fuel-core/pull/1542): Migrates information about peers to NodeInfo instead of ChainInfo. It also elides information about peers in the default node_info query.

## [Version 0.21.0]

This release focuses on preparing `fuel-core` for the mainnet environment:
- Most of the changes improved the security and stability of the node.
- The gas model was reworked to cover all aspects of execution.
- The benchmarking system was significantly enhanced, covering worst scenarios.
- A new set of benchmarks was added to track the accuracy of gas prices.
- Optimized heavy operations and removed/replaced exploitable functionality.

Besides that, there are more concrete changes:
- Unified naming conventions for all CLI arguments. Added dependencies between related fields to avoid misconfiguration in case of missing arguments. Added `--debug` flag that enables additional functionality like a debugger.
- Improved telemetry to cover the internal work of services and added support for the Pyroscope, allowing it to generate real-time flamegraphs to track performance.
- Improved stability of the P2P layer and adjusted the updating of reputation. The speed of block synchronization was significantly increased.
- The node is more stable and resilient. Improved DoS resistance and resource management. Fixed critical bugs during state transition.
- Reworked the `Mint` transaction to accumulate the fee from block production inside the contract defined by the block producer.

FuelVM received a lot of safety and stability improvements:
- The audit helped identify some bugs and errors that have been successfully fixed.
- Updated the gas price model to charge for resources used during the transaction lifecycle.
- Added `no_std` and 32 bit system support. This opens doors for fraud proving in the future.
- Removed the `ChainId` from the `PredicateId` calculation, allowing the use of predicates cross-chain.
- Improvements in the performance of some storage-related opcodes.
- Support the `ECAL` instruction that allows adding custom functionality to the VM. It can be used to create unique rollups or advanced indexers in the future.
- Support of [transaction policies](https://github.com/FuelLabs/fuel-vm/blob/master/CHANGELOG.md#version-0420) provides additional safety for the user. 
    It also allows the implementation of a multi-dimensional price model in the future, making the transaction execution cheaper and allowing more transactions that don't affect storage.
- Refactored errors, returning more detailed errors to the user, simplifying debugging.

### Added

- [#1503](https://github.com/FuelLabs/fuel-core/pull/1503): Add `gtf` opcode sanity check.
- [#1502](https://github.com/FuelLabs/fuel-core/pull/1502): Added price benchmark for `vm_initialization`.
- [#1501](https://github.com/FuelLabs/fuel-core/pull/1501): Add a CLI command for generating a fee collection contract.
- [#1492](https://github.com/FuelLabs/fuel-core/pull/1492): Support backward iteration in the RocksDB. It allows backward queries that were not allowed before.
- [#1490](https://github.com/FuelLabs/fuel-core/pull/1490): Add push and pop benchmarks.
- [#1485](https://github.com/FuelLabs/fuel-core/pull/1485): Prepare rc release of fuel core v0.21
- [#1476](https://github.com/FuelLabs/fuel-core/pull/1453): Add the majority of the "other" benchmarks for contract opcodes.
- [#1473](https://github.com/FuelLabs/fuel-core/pull/1473): Expose fuel-core version as a constant
- [#1469](https://github.com/FuelLabs/fuel-core/pull/1469): Added support of bloom filter for RocksDB tables and increased the block cache.
- [#1465](https://github.com/FuelLabs/fuel-core/pull/1465): Improvements for keygen cli and crates
- [#1642](https://github.com/FuelLabs/fuel-core/pull/1462): Added benchmark to measure the performance of contract state and contract ID calculation; use for gas costing.
- [#1457](https://github.com/FuelLabs/fuel-core/pull/1457): Fixing incorrect measurement for fast(µs) opcodes.
- [#1456](https://github.com/FuelLabs/fuel-core/pull/1456): Added flushing of the RocksDB during a graceful shutdown.
- [#1456](https://github.com/FuelLabs/fuel-core/pull/1456): Added more logs to track the service lifecycle.
- [#1453](https://github.com/FuelLabs/fuel-core/pull/1453): Add the majority of the "sanity" benchmarks for contract opcodes.
- [#1452](https://github.com/FuelLabs/fuel-core/pull/1452): Added benchmark to measure the performance of contract root calculation when utilizing the maximum contract size; used for gas costing of contract root during predicate owner validation.
- [#1449](https://github.com/FuelLabs/fuel-core/pull/1449): Fix coin pagination in e2e test client.
- [#1447](https://github.com/FuelLabs/fuel-core/pull/1447): Add timeout for continuous e2e tests
- [#1444](https://github.com/FuelLabs/fuel-core/pull/1444): Add "sanity" benchmarks for memory opcodes.
- [#1437](https://github.com/FuelLabs/fuel-core/pull/1437): Add some transaction throughput tests for basic transfers.
- [#1436](https://github.com/FuelLabs/fuel-core/pull/1436): Add a github action to continuously test beta-4.
- [#1433](https://github.com/FuelLabs/fuel-core/pull/1433): Add "sanity" benchmarks for flow opcodes.
- [#1432](https://github.com/FuelLabs/fuel-core/pull/1432): Add a new `--api-request-timeout` argument to control TTL for GraphQL requests.
- [#1430](https://github.com/FuelLabs/fuel-core/pull/1430): Add "sanity" benchmarks for crypto opcodes.
- [#1426](https://github.com/FuelLabs/fuel-core/pull/1426) Split keygen into a create and a binary.
- [#1419](https://github.com/FuelLabs/fuel-core/pull/1419): Add additional "sanity" benchmarks for arithmetic op code instructions.
- [#1411](https://github.com/FuelLabs/fuel-core/pull/1411): Added WASM and `no_std` compatibility.
- [#1405](https://github.com/FuelLabs/fuel-core/pull/1405): Use correct names for service metrics.
- [#1400](https://github.com/FuelLabs/fuel-core/pull/1400): Add releasy beta to fuel-core so that new commits to fuel-core master triggers fuels-rs.
- [#1371](https://github.com/FuelLabs/fuel-core/pull/1371): Add new client function for querying the `MessageStatus` for a specific message (by `Nonce`).
- [#1356](https://github.com/FuelLabs/fuel-core/pull/1356): Add peer reputation reporting to heartbeat code.
- [#1355](https://github.com/FuelLabs/fuel-core/pull/1355): Added new metrics related to block importing, such as tps, sync delays etc.
- [#1339](https://github.com/FuelLabs/fuel-core/pull/1339): Adds `baseAssetId` to `FeeParameters` in the GraphQL API.
- [#1331](https://github.com/FuelLabs/fuel-core/pull/1331): Add peer reputation reporting to block import code.
- [#1324](https://github.com/FuelLabs/fuel-core/pull/1324): Added pyroscope profiling to fuel-core, intended to be used by a secondary docker image that has debug symbols enabled.
- [#1309](https://github.com/FuelLabs/fuel-core/pull/1309): Add documentation for running debug builds with CLion and Visual Studio Code.  
- [#1308](https://github.com/FuelLabs/fuel-core/pull/1308): Add support for loading .env files when compiling with the `env` feature. This allows users to conveniently supply CLI arguments in a secure and IDE-agnostic way. 
- [#1304](https://github.com/FuelLabs/fuel-core/pull/1304): Implemented `submit_and_await_commit_with_receipts` method for `FuelClient`.
- [#1286](https://github.com/FuelLabs/fuel-core/pull/1286): Include readable names for test cases where missing.
- [#1274](https://github.com/FuelLabs/fuel-core/pull/1274): Added tests to benchmark block synchronization.
- [#1263](https://github.com/FuelLabs/fuel-core/pull/1263): Add gas benchmarks for `ED19` and `ECR1` instructions.

### Changed

- [#1512](https://github.com/FuelLabs/fuel-core/pull/1512): Internally simplify merkle_contract_state_range.
- [#1507](https://github.com/FuelLabs/fuel-core/pull/1507): Updated chain configuration to be ready for beta 5 network. It includes opcode prices from the latest benchmark and contract for the block producer.
- [#1477](https://github.com/FuelLabs/fuel-core/pull/1477): Upgraded the Rust version used in CI and containers to 1.73.0. Also includes associated Clippy changes.
- [#1469](https://github.com/FuelLabs/fuel-core/pull/1469): Replaced usage of `MemoryTransactionView` by `Checkpoint` database in the benchmarks.
- [#1468](https://github.com/FuelLabs/fuel-core/pull/1468): Bumped version of the `fuel-vm` to `v0.40.0`. It brings some breaking changes into consensus parameters API because of changes in the underlying types.
- [#1466](https://github.com/FuelLabs/fuel-core/pull/1466): Handling overflows during arithmetic operations.
- [#1460](https://github.com/FuelLabs/fuel-core/pull/1460): Change tracking branch from main to master for releasy tests.
- [#1454](https://github.com/FuelLabs/fuel-core/pull/1454): Update gas benchmarks for opcodes that append receipts.
- [#1440](https://github.com/FuelLabs/fuel-core/pull/1440): Don't report reserved nodes that send invalid transactions.
- [#1439](https://github.com/FuelLabs/fuel-core/pull/1439): Reduced memory BMT consumption during creation of the header.
- [#1434](https://github.com/FuelLabs/fuel-core/pull/1434): Continue gossiping transactions to reserved peers regardless of gossiping reputation score.
- [#1408](https://github.com/FuelLabs/fuel-core/pull/1408): Update gas benchmarks for storage opcodes to use a pre-populated database to get more accurate worst-case costs.
- [#1399](https://github.com/FuelLabs/fuel-core/pull/1399): The Relayer now queries Ethereum for its latest finalized block instead of using a configurable "finalization period" to presume finality.
- [#1397](https://github.com/FuelLabs/fuel-core/pull/1397): Improved keygen. Created a crate to be included from forc plugins and upgraded internal library to drop requirement of protoc to build
- [#1395](https://github.com/FuelLabs/fuel-core/pull/1395): Add DependentCost benchmarks for `k256`, `s256` and `mcpi` instructions.
- [#1393](https://github.com/FuelLabs/fuel-core/pull/1393): Increase heartbeat timeout from `2` to `60` seconds, as suggested in [this issue](https://github.com/FuelLabs/fuel-core/issues/1330).
- [#1392](https://github.com/FuelLabs/fuel-core/pull/1392): Fixed an overflow in `message_proof`.
- [#1390](https://github.com/FuelLabs/fuel-core/pull/1390): Up the `ethers` version to `2` to fix an issue with `tungstenite`.
- [#1383](https://github.com/FuelLabs/fuel-core/pull/1383): Disallow usage of `log` crate internally in favor of `tracing` crate.
- [#1380](https://github.com/FuelLabs/fuel-core/pull/1380): Add preliminary, hard-coded config values for heartbeat peer reputation, removing `todo`.
- [#1377](https://github.com/FuelLabs/fuel-core/pull/1377): Remove `DiscoveryEvent` and use `KademliaEvent` directly in `DiscoveryBehavior`.
- [#1366](https://github.com/FuelLabs/fuel-core/pull/1366): Improve caching during docker builds in CI by replacing gha
- [#1358](https://github.com/FuelLabs/fuel-core/pull/1358): Upgraded the Rust version used in CI to 1.72.0. Also includes associated Clippy changes.
- [#1349](https://github.com/FuelLabs/fuel-core/pull/1349): Updated peer-to-peer transactions API to support multiple blocks in a single request, and updated block synchronization to request multiple blocks based on the configured range of headers.
- [#1342](https://github.com/FuelLabs/fuel-core/pull/1342): Add error handling for P2P requests to return `None` to requester and log error.
- [#1318](https://github.com/FuelLabs/fuel-core/pull/1318): Modified block synchronization to use asynchronous task execution when retrieving block headers.
- [#1314](https://github.com/FuelLabs/fuel-core/pull/1314): Removed `types::ConsensusParameters` in favour of `fuel_tx:ConsensusParameters`.
- [#1302](https://github.com/FuelLabs/fuel-core/pull/1302): Removed the usage of flake and building of the bridge contract ABI.
    It simplifies the maintenance and updating of the events, requiring only putting the event definition into the codebase of the relayer.
- [#1293](https://github.com/FuelLabs/fuel-core/issues/1293): Parallelized the `estimate_predicates` endpoint to utilize all available threads.
- [#1270](https://github.com/FuelLabs/fuel-core/pull/1270): Modify the way block headers are retrieved from peers to be done in batches.

#### Breaking
- [#1506](https://github.com/FuelLabs/fuel-core/pull/1506): Added validation of the coin's fields during block production and validation. Before, it was possible to submit a transaction that didn't match the coin's values in the database, allowing printing/using unavailable assets.
- [#1491](https://github.com/FuelLabs/fuel-core/pull/1491): Removed unused request and response variants from the Gossipsub implementation, as well as related definitions and tests. Specifically, this removes gossiping of `ConsensusVote` and `NewBlock` events.
- [#1472](https://github.com/FuelLabs/fuel-core/pull/1472): Upgraded `fuel-vm` to `v0.42.0`. It introduces transaction policies that changes layout of the transaction. FOr more information check the [v0.42.0](https://github.com/FuelLabs/fuel-vm/pull/635) release.
- [#1470](https://github.com/FuelLabs/fuel-core/pull/1470): Divide `DependentCost` into "light" and "heavy" operations.
- [#1464](https://github.com/FuelLabs/fuel-core/pull/1464): Avoid possible truncation of higher bits. It may invalidate the code that truncated higher bits causing different behavior on 32-bit vs. 64-bit systems. The change affects some endpoints that now require lesser integers.
- [#1432](https://github.com/FuelLabs/fuel-core/pull/1432): All subscriptions and requests have a TTL now. So each subscription lifecycle is limited in time. If the subscription is closed because of TTL, it means that you subscribed after your transaction had been dropped by the network.
- [#1407](https://github.com/FuelLabs/fuel-core/pull/1407): The recipient is a `ContractId` instead of `Address`. The block producer should deploy its contract to receive the transaction fee. The collected fee is zero until the recipient contract is set.
- [#1407](https://github.com/FuelLabs/fuel-core/pull/1407): The `Mint` transaction is reworked with new fields to support the account-base model. It affects serialization and deserialization of the transaction and also affects GraphQL schema.
- [#1407](https://github.com/FuelLabs/fuel-core/pull/1407): The `Mint` transaction is the last transaction in the block instead of the first.
- [#1374](https://github.com/FuelLabs/fuel-core/pull/1374): Renamed `base_chain_height` to `da_height` and return current relayer height instead of latest Fuel block height.
- [#1367](https://github.com/FuelLabs/fuel-core/pull/1367): Update to the latest version of fuel-vm.
- [#1363](https://github.com/FuelLabs/fuel-core/pull/1363): Change message_proof api to take `nonce` instead of `message_id`
- [#1355](https://github.com/FuelLabs/fuel-core/pull/1355): Removed the `metrics` feature flag from the fuel-core crate, and metrics are now included by default.
- [#1339](https://github.com/FuelLabs/fuel-core/pull/1339): Added a new required field called `base_asset_id` to the `FeeParameters` definition in `ConsensusParameters`, as well as default values for `base_asset_id` in the `beta` and `dev` chain specifications.
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
- [#1484](https://github.com/FuelLabs/fuel-core/pull/1484): Removed `--network` CLI argument. Now the name of the network is fetched form chain configuration.
- [#1399](https://github.com/FuelLabs/fuel-core/pull/1399): Removed `relayer-da-finalization` parameter from the relayer CLI.
- [#1338](https://github.com/FuelLabs/fuel-core/pull/1338): Updated GraphQL client to use `DependentCost` for `k256`, `mcpi`, `s256`, `scwq`, `swwq` opcodes.
- [#1322](https://github.com/FuelLabs/fuel-core/pull/1322): The `manual_blocks_enabled` flag is removed from the CLI. The analog is a `debug` flag.