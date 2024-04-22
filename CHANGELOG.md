# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

Description of the upcoming release here.

### Added

- [#1853](https://github.com/FuelLabs/fuel-core/pull/1853): Added a test case to verify the database's behavior when new columns are added to the RocksDB database.

## [Version 0.25.2]

### Fixed

- [#1844](https://github.com/FuelLabs/fuel-core/pull/1844): Fixed the publishing of the `fuel-core 0.25.1` release.
- [1842](https://github.com/FuelLabs/fuel-core/pull/1842): Ignore RUSTSEC-2024-0336: `rustls::ConnectionCommon::complete_io` could fall into an infinite loop based on network

## [Version 0.25.1]

### Fixed

- [#1840](https://github.com/FuelLabs/fuel-core/pull/1840): Fixed the publishing of the `fuel-core 0.25.0` release.

## [Version 0.25.0]

### Fixed

- [#1821](https://github.com/FuelLabs/fuel-core/pull/1821): Can handle missing tables in snapshot.
- [#1814](https://github.com/FuelLabs/fuel-core/pull/1814): Bugfix: the `iter_all_by_prefix` was not working for all tables. The change adds a `Rust` level filtering.

### Added

- [#1831](https://github.com/FuelLabs/fuel-core/pull/1831): Included the total gas and fee used by transaction into `TransactionStatus`.
- [#1821](https://github.com/FuelLabs/fuel-core/pull/1821): Propagate shutdown signal to (re)genesis. Also add progress bar for (re)genesis.
- [#1813](https://github.com/FuelLabs/fuel-core/pull/1813): Added back support for `/health` endpoint.
- [#1799](https://github.com/FuelLabs/fuel-core/pull/1799): Snapshot creation is now concurrent.
- [#1811](https://github.com/FuelLabs/fuel-core/pull/1811): Regenesis now preserves old blocks and transactions for GraphQL API.

### Changed

- [#1833](https://github.com/FuelLabs/fuel-core/pull/1833): Regenesis of `SpentMessages` and `ProcessedTransactions`.
- [#1830](https://github.com/FuelLabs/fuel-core/pull/1830): Use versioning enum for WASM executor input and output.
- [#1816](https://github.com/FuelLabs/fuel-core/pull/1816): Updated the upgradable executor to fetch the state transition bytecode from the database when the version doesn't match a native one. This change enables the WASM executor in the "production" build and requires a `wasm32-unknown-unknown` target.
- [#1812](https://github.com/FuelLabs/fuel-core/pull/1812): Follow-up PR to simplify the logic around parallel snapshot creation.
- [#1809](https://github.com/FuelLabs/fuel-core/pull/1809): Fetch `ConsensusParameters` from the database
- [#1808](https://github.com/FuelLabs/fuel-core/pull/1808): Fetch consensus parameters from the provider.

#### Breaking

- [#1826](https://github.com/FuelLabs/fuel-core/pull/1826): The changes make the state transition bytecode part of the `ChainConfig`. It guarantees the state transition's availability for the network's first blocks.
    The change has many minor improvements in different areas related to the state transition bytecode:
    - The state transition bytecode lies in its own file(`state_transition_bytecode.wasm`) along with the chain config file. The `ChainConfig` loads it automatically when `ChainConfig::load` is called and pushes it back when `ChainConfig::write` is called.
    - The `fuel-core` release bundle also contains the `fuel-core-wasm-executor.wasm` file of the corresponding executor version.
    - The regenesis process now considers the last block produced by the previous network. When we create a (re)genesis block of a new network, it has the `height = last_block_of_old_netowkr + 1`. It continues the old network and doesn't overlap blocks(before, we had `old_block.height == new_genesis_block.hegiht`).
    - Along with the new block height, the regenesis process also increases the state transition bytecode and consensus parameters versions. It guarantees that a new network doesn't use values from the previous network and allows us not to migrate `StateTransitionBytecodeVersions` and `ConsensusParametersVersions` tables.
    - Added a new CLI argument, `native-executor-version,` that allows overriding of the default version of the native executor. It can be useful for side rollups that have their own history of executor upgrades.
    - Replaced:
      
      ```rust
               let file = std::fs::File::open(path)?;
               let mut snapshot: Self = serde_json::from_reader(&file)?;
      ```
      
      with a:
      
      ```rust
               let mut json = String::new();
               std::fs::File::open(&path)
                   .with_context(|| format!("Could not open snapshot file: {path:?}"))?
                   .read_to_string(&mut json)?;
               let mut snapshot: Self = serde_json::from_str(json.as_str())?;
      ```
      because it is 100 times faster for big JSON files.
    - Updated all tests to use `Config::local_node_*` instead of working with the `SnapshotReader` directly. It is the preparation of the tests for the futures bumps of the `Executor::VERSION`. When we increase the version, all tests continue to use `GenesisBlock.state_transition_bytecode = 0` while the version is different, which forces the usage of the WASM executor, while for tests, we still prefer to test native execution. The `Config::local_node_*` handles it and forces the executor to use the native version.
    - Reworked the `build.rs` file of the upgradable executor. The script now caches WASM bytecode to avoid recompilation. Also, fixed the issue with outdated WASM bytecode. The script reacts on any modifications of the `fuel-core-wasm-executor` and forces recompilation (it is why we need the cache), so WASM bytecode always is actual now.
- [#1822](https://github.com/FuelLabs/fuel-core/pull/1822): Removed support of `Create` transaction from debugger since it doesn't have any script to execute.
- [#1822](https://github.com/FuelLabs/fuel-core/pull/1822): Use `fuel-vm 0.49.0` with new transactions types - `Upgrade` and `Upload`. Also added `max_bytecode_subsections` field to the `ConsensusParameters` to limit the number of bytecode subsections in the state transition bytecode. 
- [#1816](https://github.com/FuelLabs/fuel-core/pull/1816): Updated the upgradable executor to fetch the state transition bytecode from the database when the version doesn't match a native one. This change enables the WASM executor in the "production" build and requires a `wasm32-unknown-unknown` target.

## [Version 0.24.2]

### Changed

#### Breaking
- [#1798](https://github.com/FuelLabs/fuel-core/pull/1798): Add nonce to relayed transactions and also hash full messages in the inbox root.

### Fixed

- [#1802](https://github.com/FuelLabs/fuel-core/pull/1802): Fixed a runtime panic that occurred when restarting a node. The panic was caused by an invalid database commit while loading an existing off-chain database. The invalid commit is removed in this PR.
- [#1803](https://github.com/FuelLabs/fuel-core/pull/1803): Produce block when da height haven't changed.
- [#1795](https://github.com/FuelLabs/fuel-core/pull/1795): Fixed the building of the `fuel-core-wasm-executor` to work outside of the `fuel-core` context. The change uses the path to the manifest file of the `fuel-core-upgradable-executor` to build the `fuel-core-wasm-executor` instead of relying on the workspace.

## [Version 0.24.1]

### Added

- [#1787](https://github.com/FuelLabs/fuel-core/pull/1787): Handle processing of relayed (forced) transactions
- [#1786](https://github.com/FuelLabs/fuel-core/pull/1786): Regenesis now includes off-chain tables.
- [#1716](https://github.com/FuelLabs/fuel-core/pull/1716): Added support of WASM state transition along with upgradable execution that works with native(std) and WASM(non-std) executors. The `fuel-core` now requires a `wasm32-unknown-unknown` target to build.
- [#1770](https://github.com/FuelLabs/fuel-core/pull/1770): Add the new L1 event type for forced transactions.
- [#1767](https://github.com/FuelLabs/fuel-core/pull/1767): Added consensus parameters version and state transition version to the `ApplicationHeader` to describe what was used to produce this block.
- [#1760](https://github.com/FuelLabs/fuel-core/pull/1760): Added tests to verify that the network operates with a custom chain id and base asset id.
- [#1752](https://github.com/FuelLabs/fuel-core/pull/1752): Add `ProducerGasPrice` trait that the `Producer` depends on to get the gas price for the block.
- [#1747](https://github.com/FuelLabs/fuel-core/pull/1747): The DA block height is now included in the genesis state.
- [#1740](https://github.com/FuelLabs/fuel-core/pull/1740): Remove optional fields from genesis configs
- [#1737](https://github.com/FuelLabs/fuel-core/pull/1737): Remove temporary tables for calculating roots during genesis.
- [#1731](https://github.com/FuelLabs/fuel-core/pull/1731): Expose `schema.sdl` from `fuel-core-client`.

### Changed

#### Breaking

- [1785](https://github.com/FuelLabs/fuel-core/pull/1785): Producer will only include DA height if it has enough gas to include the associate forced transactions.
- [#1771](https://github.com/FuelLabs/fuel-core/pull/1771): Contract 'states' and 'balances' brought back into `ContractConfig`. Parquet now writes a file per table.
- [1779](https://github.com/FuelLabs/fuel-core/pull/1779): Modify Relayer service to order Events from L1 by block index
- [#1783](https://github.com/FuelLabs/fuel-core/pull/1783): The PR upgrade `fuel-vm` to `0.48.0` release. Because of some breaking changes, we also adapted our codebase to follow them: 
  - Implementation of `Default` for configs was moved under the `test-helpers` feature. The `fuel-core` binary uses testnet configuration instead of `Default::default`(for cases when `ChainConfig` was not provided by the user).
  - All parameter types are enums now and require corresponding modifications across the codebase(we need to use getters and setters). The GraphQL API remains the same for simplicity, but each parameter now has one more field - `version`, that can be used to decide how to deserialize. 
  - The `UtxoId` type now is 34 bytes instead of 33. It affects hex representation and requires adding `00`.
  - The `block_gas_limit` was moved to `ConsensusParameters` from `ChainConfig`. It means the block producer doesn't specify the block gas limit anymore, and we don't need to propagate this information.
  - The `bytecodeLength` field is removed from the `Create` transaction.
  - Removed `ConsensusParameters` from executor config because `ConsensusParameters::default` is not available anymore. Instead, executors fetch `ConsensusParameters` from the database.

- [#1769](https://github.com/FuelLabs/fuel-core/pull/1769): Include new field on header for the merkle root of imported events. Rename other message root field.
- [#1768](https://github.com/FuelLabs/fuel-core/pull/1768): Moved `ContractsInfo` table to the off-chain database. Removed `salt` field from the `ContractConfig`.
- [#1761](https://github.com/FuelLabs/fuel-core/pull/1761): Adjustments to the upcoming testnet configs:
  - Decreased the max size of the contract/predicate/script to be 100KB.
  - Decreased the max size of the transaction to be 110KB.
  - Decreased the max number of storage slots to be 1760(110KB / 64).
  - Removed fake coins from the genesis state.
  - Renamed folders to be "testnet" and "dev-testnet".
  - The name of the networks are "Upgradable Testnet" and "Upgradable Dev Testnet".

- [#1694](https://github.com/FuelLabs/fuel-core/pull/1694): The change moves the database transaction logic from the `fuel-core` to the `fuel-core-storage` level. The corresponding [issue](https://github.com/FuelLabs/fuel-core/issues/1589) described the reason behind it.

    ## Technical details of implementation

    - The change splits the `KeyValueStore` into `KeyValueInspect` and `KeyValueMutate`, as well the `Blueprint` into `BlueprintInspect` and `BlueprintMutate`. It allows requiring less restricted constraints for any read-related operations.

    - One of the main ideas of the change is to allow for the actual storage only to implement `KeyValueInspect` and `Modifiable` without the `KeyValueMutate`. It simplifies work with the databases and provides a safe way of interacting with them (Modification into the database can only go through the `Modifiable::commit_changes`). This feature is used to [track the height](https://github.com/FuelLabs/fuel-core/pull/1694/files#diff-c95a3d57a39feac7c8c2f3b193a24eec39e794413adc741df36450f9a4539898) of each database during commits and even limit how commits are done, providing additional safety. This part of the change was done as a [separate commit](https://github.com/FuelLabs/fuel-core/pull/1694/commits/7b1141ac838568e3590f09dd420cb24a6946bd32).
    
    - The `StorageTransaction` is a `StructuredStorage` that uses `InMemoryTransaction` inside to accumulate modifications. Only `InMemoryTransaction` has a real implementation of the `KeyValueMutate`(Other types only implement it in tests).
    
    - The implementation of the `Modifiable` for the `Database` contains a business logic that provides additional safety but limits the usage of the database. The `Database` now tracks its height and is responsible for its updates. In the `commit_changes` function, it analyzes the changes that were done and tries to find a new height(For example, in the case of the `OnChain` database, we are looking for a new `Block` in the `FuelBlocks` table).
    
    - As was planned in the issue, now the executor has full control over how commits to the storage are done.
    
    - All mutation methods now require `&mut self` - exclusive ownership over the object to be able to write into it. It almost negates the chance of concurrent modification of the storage, but it is still possible since the `Database` implements the `Clone` trait. To be sure that we don't corrupt the state of the database, the `commit_changes` function implements additional safety checks to be sure that we commit updates per each height only once time.

    - Side changes:
      - The `drop` function was moved from `Database` to `RocksDB` as a preparation for the state rewind since the read view should also keep the drop function until it is destroyed.
      - The `StatisticTable` table lives in the off-chain worker.
      - Removed duplication of the `Database` from the `dap::ConcreteStorage` since it is already available from the VM.
      - The executor return only produced `Changes` instead of the storage transaction, which simplifies the interaction between modules and port definition.
      - The logic related to the iteration over the storage is moved to the `fuel-core-storage` crate and is now reusable. It provides an `interator` method that duplicates the logic from `MemoryStore` on iterating over the `BTreeMap` and methods like `iter_all`, `iter_all_by_prefix`, etc. It was done in a separate revivable [commit](https://github.com/FuelLabs/fuel-core/pull/1694/commits/5b9bd78320e6f36d0650ec05698f12f7d1b3c7c9).
      - The `MemoryTransactionView` is fully replaced by the `StorageTransactionInner`.
      - Removed `flush` method from the `Database` since it is not needed after https://github.com/FuelLabs/fuel-core/pull/1664.

- [#1693](https://github.com/FuelLabs/fuel-core/pull/1693): The change separates the initial chain state from the chain config and stores them in separate files when generating a snapshot. The state snapshot can be generated in a new format where parquet is used for compression and indexing while postcard is used for encoding. This enables importing in a stream like fashion which reduces memory requirements. Json encoding is still supported to enable easy manual setup. However, parquet is prefered for large state files.

  ### Snapshot command

  The CLI was expanded to allow customizing the used encoding. Snapshots are now generated along with a metadata file describing the encoding used. The metadata file contains encoding details as well as the location of additional files inside the snapshot directory containing the actual data. The chain config is always generated in the JSON format.

  The snapshot command now has the '--output-directory' for specifying where to save the snapshot.

  ### Run command

  The run command now includes the 'db_prune' flag which when provided will prune the existing db and start genesis from the provided snapshot metadata file or the local testnet configuration.

  The snapshot metadata file contains paths to the chain config file and files containing chain state items (coins, messages, contracts, contract states, and balances), which are loaded via streaming.

  Each item group in the genesis process is handled by a separate worker, allowing for parallel loading. Workers stream file contents in batches.

  A database transaction is committed every time an item group is succesfully loaded. Resumability is achieved by recording the last loaded group index within the same db tx. If loading is aborted, the remaining workers are shutdown. Upon restart, workers resume from the last processed group.

  ### Contract States and Balances

  Using uniform-sized batches may result in batches containing items from multiple contracts. Optimal performance can presumably be achieved by selecting a batch size that typically encompasses an entire contract's state or balance, allowing for immediate initialization of relevant Merkle trees.

### Removed

- [#1757](https://github.com/FuelLabs/fuel-core/pull/1757): Removed `protobuf` from everywhere since `libp2p` uses `quick-protobuf`.

## [Version 0.23.0]

### Added

- [#1713](https://github.com/FuelLabs/fuel-core/pull/1713): Added automatic `impl` of traits `StorageWrite` and `StorageRead` for `StructuredStorage`. Tables that use a `Blueprint` can be read and written using these interfaces provided by structured storage types.
- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): Added a new `Merklized` blueprint that maintains the binary Merkle tree over the storage data. It supports only the insertion of the objects without removing them.
- [#1657](https://github.com/FuelLabs/fuel-core/pull/1657): Moved `ContractsInfo` table from `fuel-vm` to on-chain tables, and created version-able `ContractsInfoType` to act as the table's data type.

### Changed

- [#1723](https://github.com/FuelLabs/fuel-core/pull/1723): Notify about imported blocks from the off-chain worker.
- [#1717](https://github.com/FuelLabs/fuel-core/pull/1717): The fix for the [#1657](https://github.com/FuelLabs/fuel-core/pull/1657) to include the contract into `ContractsInfo` table.
- [#1657](https://github.com/FuelLabs/fuel-core/pull/1657): Upgrade to `fuel-vm` 0.46.0.
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

- [#1725](https://github.com/FuelLabs/fuel-core/pull/1725): All API endpoints now are prefixed with `/v1` version. New usage looks like: `/v1/playground`, `/v1/graphql`, `/v1/graphql-sub`, `/v1/metrics`, `/v1/health`.
- [#1722](https://github.com/FuelLabs/fuel-core/pull/1722): Bugfix: Zero `predicate_gas_used` field during validation of the produced block.
- [#1714](https://github.com/FuelLabs/fuel-core/pull/1714): The change bumps the `fuel-vm` to `0.47.1`. It breaks several breaking changes into the protocol:
  - All malleable fields are zero during the execution and unavailable through the GTF getters. Accessing them via the memory directly is still possible, but they are zero.
  - The `Transaction` doesn't define the gas price anymore. The gas price is defined by the block producer and recorded in the `Mint` transaction at the end of the block. A price of future blocks can be fetched through a [new API nedopoint](https://github.com/FuelLabs/fuel-core/issues/1641) and the price of the last block can be fetch or via the block or another [API endpoint](https://github.com/FuelLabs/fuel-core/issues/1647).
  - The `GasPrice` policy is replaced with the `Tip` policy. The user may specify in the native tokens how much he wants to pay the block producer to include his transaction in the block. It is the prioritization mechanism to incentivize the block producer to include users transactions earlier.
  - The `MaxFee` policy is mandatory to set. Without it, the transaction pool will reject the transaction. Since the block producer defines the gas price, the only way to control how much user agreed to pay can be done only through this policy.
  - The `maturity` field is removed from the `Input::Coin`. The same affect can be achieve with the `Maturity` policy on the transaction and predicate. This changes breaks how input coin is created and removes the passing of this argument.
  - The metadata of the `Checked<Tx>` doesn't contain `max_fee` and `min_fee` anymore. Only `max_gas` and `min_gas`. The `max_fee` is controlled by the user via the `MaxFee` policy.
  - Added automatic `impl` of traits `StorageWrite` and `StorageRead` for `StructuredStorage`. Tables that use a `Blueprint` can be read and written using these interfaces provided by structured storage types.

- [#1712](https://github.com/FuelLabs/fuel-core/pull/1712): Make `ContractUtxoInfo` type a version-able enum for use in the `ContractsLatestUtxo`table.
- [#1657](https://github.com/FuelLabs/fuel-core/pull/1657): Changed `CROO` gas price type from `Word` to `DependentGasPrice`. The dependent gas price values are dummy values while awaiting updated benchmarks.
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


## [Version 0.22.4]

### Added

- [#1743](https://github.com/FuelLabs/fuel-core/pull/1743): Added blacklisting of the transactions on the `TxPool` level.
  ```shell
        --tx-blacklist-addresses <TX_BLACKLIST_ADDRESSES>
            The list of banned addresses ignored by the `TxPool`
            
            [env: TX_BLACKLIST_ADDRESSES=]
  
        --tx-blacklist-coins <TX_BLACKLIST_COINS>
            The list of banned coins ignored by the `TxPool`
            
            [env: TX_BLACKLIST_COINS=]
  
        --tx-blacklist-messages <TX_BLACKLIST_MESSAGES>
            The list of banned messages ignored by the `TxPool`
            
            [env: TX_BLACKLIST_MESSAGES=]
  
        --tx-blacklist-contracts <TX_BLACKLIST_CONTRACTS>
            The list of banned contracts ignored by the `TxPool`
            
            [env: TX_BLACKLIST_CONTRACTS=]
  ```

## [Version 0.22.3]

### Added

- [#1732](https://github.com/FuelLabs/fuel-core/pull/1732): Added `Clone` bounds to most datatypes of `fuel-core-client`.

## [Version 0.22.2]

### Added

- [#1729](https://github.com/FuelLabs/fuel-core/pull/1729): Exposed the `schema.sdl` file from `fuel-core-client`. The user can create his own queries by using this file.

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
