# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased (see .changes folder)]

## [Version 0.42.0]

### Breaking
- [2648](https://github.com/FuelLabs/fuel-core/pull/2648): Add feature-flagged field to block header  that contains a commitment to all transaction ids.
- [2678](https://github.com/FuelLabs/fuel-core/pull/2678): Removed public accessors for  fields and replaced with methods instead, moved  to the application header of .
- [2746](https://github.com/FuelLabs/fuel-core/pull/2746): Breaking changes to the CLI arguments: - To disable  just don't specify it. Before it required to use . - Next CLI arguments were renamed: -  ->  -  ->  -  ->  - Default value for the  was changed from  to . So information about new block will be propagated faster by default. - All CLI arguments below use time(like , , , etc.) use as a flag argument instead of number of seconds: -  -  -  -  -  -  -  -  -  -  -  -  -  - 
- [2840](https://github.com/FuelLabs/fuel-core/pull/2840): CLI argument  is deprecated and does nothing. It will be removed in a future version of . The  field was renamed into  that affects JSON based serialization/deserialization. Renamed  field into .

### Added
- [2150](https://github.com/FuelLabs/fuel-core/pull/2150): Upgraded  to  and introduced  to limit pending incoming/outgoing connections.
- [2491](https://github.com/FuelLabs/fuel-core/pull/2491): Storage read replays of historical blocks for execution tracing. Only available behind  flag.
- [2619](https://github.com/FuelLabs/fuel-core/pull/2619): Add possibility to submit list of changes to rocksdb.
- [2666](https://github.com/FuelLabs/fuel-core/pull/2666): Added two new CLI arguments to control the GraphQL queries consistency:  (default: ) and  (default: ). If a request requires a specific block height and the node is slightly behind, it will wait instead of failing.
- [2682](https://github.com/FuelLabs/fuel-core/pull/2682): Added GraphQL APIs to get contract storage and balances for current and past blocks.
- [2719](https://github.com/FuelLabs/fuel-core/pull/2719): Merklized DA compression temporal registry tables.
- [2722](https://github.com/FuelLabs/fuel-core/pull/2722): Service definition for state root service.
- [2724](https://github.com/FuelLabs/fuel-core/pull/2724): Explicit error type for merkleized storage.
- [2726](https://github.com/FuelLabs/fuel-core/pull/2726): Add a new gossip-sub message for transaction preconfirmations
- [2731](https://github.com/FuelLabs/fuel-core/pull/2731): Include  trait implementations for v2 tables.
- [2733](https://github.com/FuelLabs/fuel-core/pull/2733): Add a pending pool transaction that allow transaction to wait a bit of time if an input is missing instead of direct delete.
- [2742](https://github.com/FuelLabs/fuel-core/pull/2742): Added API crate for merkle root service.
- [2756](https://github.com/FuelLabs/fuel-core/pull/2756): Add new service for managing pre-confirmations
- [2769](https://github.com/FuelLabs/fuel-core/pull/2769): Added a new  GraphQL endpoint. The endpoint can be used to assemble the transaction based on the provided requirements. The returned transaction contains: - Input coins to cover  - Input coins to cover the fee of the transaction based on the gas price from  -  or  outputs for all assets from the inputs -  outputs in the case they are required during the execution -  inputs and outputs in the case they are required during the execution - Reserved witness slots for signed coins filled with  zeroes - Set script gas limit(unless Script started, output log file is 'typescript'.

## [Version 0.41.9]

### Fixed

- [2829](https://github.com/FuelLabs/fuel-core/pull/2829): Ensure that the local state of `fuel-core-relayer` is set correctly after downloading logs from DA.
- [2830](https://github.com/FuelLabs/fuel-core/pull/2830): Ensure that the block producer _only_ starts after all the services have been initialized.

## [Version 0.41.8]

### Fixed

- [2809](https://github.com/FuelLabs/fuel-core/pull/2809): When the state rewind window of 7d was triggered, the `is_migration_in_progress` was repeatedly called, resulting in multiple iterations over the empty ModificationsHistoryV1 table. Iteration was slow because compaction didn't have a chance to clean up V1 table. We removed iteration from the migration process.


## [Version 0.41.7]

### Fixed
- [2710](https://github.com/FuelLabs/fuel-core/pull/2710): Update Fuel-VM to fix compressed transaction backward compatibility.

## [Version 0.41.6]

### Added
- [2668](https://github.com/FuelLabs/fuel-core/pull/2668): Expose gas price service test helpers
- [2621](https://github.com/FuelLabs/fuel-core/pull/2598): Global merkle root storage updates process upgrade transactions.
- [2650](https://github.com/FuelLabs/fuel-core/pull/2650): Populate `ProcessedTransactions` table in global merkle root storage.
- [2667](https://github.com/FuelLabs/fuel-core/pull/2667): Populate `Blobs` table in global merkle root storage.
- [2652](https://github.com/FuelLabs/fuel-core/pull/2652): Global Merkle Root storage crate: Add Raw contract bytecode to global merkle root storage when processing Create transactions.
- [2669](https://github.com/FuelLabs/fuel-core/pull/2669): Populate `UploadedBytecodes` table in global merkle root storage.

### Fixed
- [2673](https://github.com/FuelLabs/fuel-core/pull/2673): Change read behavior on the InMemoryTransaction to use offset and allow not equal buf size (fix CCP and LDC broken from https://github.com/FuelLabs/fuel-vm/pull/847)

## [Version 0.41.5]

### Changed
- [2387](https://github.com/FuelLabs/fuel-core/pull/2387): Update description `tx-max-depth` flag.
- [2630](https://github.com/FuelLabs/fuel-core/pull/2630): Removed some noisy `tracing::info!` logs
- [2643](https://github.com/FuelLabs/fuel-core/pull/2643): B$ efore this fix when tip is zero, transactions that use 30M h$ ave the same priority as transactions $ with 1M gas. Now they are correctly or$ dered.
- [2645](https://github.com/F$ uelLabs/fuel-core/pull/2645): Refacto$ red `cumulative_percent_change` for 90% perf gains and reduced duplication.

### Breakish: 6: ng
- [2661](httpsSyntax error: "(" unexpected://github.com/FuelLabs/fuel-core/pull/2661): Dry r
un now supports running in past blo$ cks. `dry_run_opt` method now takes block number as the lash: 6: st argument. To retainSyntax error: "(" unexpected old behavior, si
mply pass in `None` for $ the last argument.

### Added
- [2617$ ](https://github.com/FuelLabs/fuel-core$ /pull/2617): Add integration skeleton $ of parallel-executor.
- [2553](https://$ github.com/FuelLabs/fuel-core/pull/2553): Sc$ affold global merkle root storage crate.
- [2598](https://gsh: 11: ithub.com/FuelLabs/Syntax error: "(" unexpectedfuel-core/pull/
2598): Add initial test $ suite for global merkle root storage u$ pdates.
- [2635](https://github.co$ m/FuelLabs/fuel-core/pull/2635): Add metric$ s to gas price service
- [2664]($ https://github.com/FuelLabs/fuel-core/pull$ /2664): Add print with all information when a sh: 16: transaction is refusSyntax error: "(" unexpecteded because of a
 collision.

### F$ ixed
- [2632](https://github.com/Fu$ elLabs/fuel-core/pull/2632): Improved $ performance of certain async trait i$ mpls in the gas price service.
- $ [2662](https://github.com/FuelLabs/fuel-core/pull/2sh: 20: 662): Fix balances query endpoint cost without indexation and behavior coins to spendSyntax error: "(" unexpected with one para
meter at zero.

## [$ Version 0.41.4]

### Fixed
- [2628]sh: 20: (https://giSyntax error: "(" unexpectedthub.co
m/FuelLabs/fuel-$ core/pull/2628): Downgrade STF.

## [Version 0.41.3sh: 20: ]

### FixedSyntax error: "(" unexpected
- [26
26](https://github.com/F$ uelLabs/fuel-core/pull/2626): Avoid needs of RocksDBsh: 20:  features in testsSyntax error: "(" unexpected modules.


## [Version 0.41.2]
$ 
### Fixed

- [2623](https://github.com/FuelLabs/fush: 20: el-core/pull/Syntax error: "(" unexpected2623): Pinne
d netlink-pro$ to's version.

## [Version 0sh: 20: .41.1]

### Added
Syntax error: "(" unexpected- [2551](https://g
ithub$ .com/FuelLabs/fuel-core/pull/2551): Enhanc$ ed the DA compressed block header to $ include block id.
- [2595](https://github.com/FuelLabssh: 22: /fuel-core/pull/2595): Added `indexation` field to the `nodeInfo` GraSyntax error: "(" unexpectedphQL endpoint 
to allow checking if a $ specific indexation is enabled.

#$ ## Changed
- [2603](https://github.com$ /FuelLabs/fuel-core/pull/2603): Set$ s the latest recorded height on initializ$ ation, not just when DA costs are received

sh: 26: ### Fixed
- Syntax error: "(" unexpected[2612](https:/
/github.com/FuelLabs/fue$ l-core/pull/2612): Use latest gas price to estsh: 26: imate next block gaSyntax error: "(" unexpecteds price durin
g dry runs
- [2612](https:$ //github.com/FuelLabs/fuel-core/pull/2612): Use latest gsh: 26: as price to estimate next block gas price in tx pool instead of using algorithm directly
- [2609](https://github.com/FuelLabs/fuel-core/pull/2609): CheSyntax error: "(" unexpectedck response be
fore trying to deseriali$ ze, return error instead
- [2599](https://github.com/sh: 26: FuelLabs/fuel-core/Syntax error: "(" unexpectedpull/2599): Us
e the proper `url` api$ s to construct full url path in `BlockCommitterHttpApi` client
- [2593](https$ ://github.com/FuelLabs/fuel-core/pull$ /2593): Fixed utxo id decompression

## [Version sh: 28: 0.41.0]

### ASyntax error: "(" unexpecteddded
- [2547](h
ttps://github.com/Fuel$ Labs/fuel-core/pull/2547): Replace$  the old Graphql gas price provider $ adapter with the ArcGasPriceEstimate.
- [244sh: 30: 5](https://github.Syntax error: "(" unexpectedcom/FuelLabs/f
uel-core/pull/2445): Added $ GQL endpoint for querying asset details.
- [24sh: 30: 42](https://githubSyntax error: "(" unexpected.com/FuelLab
s/fuel-core/pull/244$ 2): Add uninitialized task for V1 gas pricesh: 30:  service
- [215Syntax error: "(" unexpected4](https://gith
ub.com/FuelLabs/fuel-co$ re/pull/2154): Added `Unknown` variant to `Cosh: 30: nsensusParametersSyntax error: "(" unexpected` graphql quer
ies
- [2154](https://$ github.com/FuelLabs/fuel-core/pull/2154): Addesh: 30: d `Unknown` varianSyntax error: "(" unexpectedt to `Block` gr
aphql queries
- [21$ 54](https://github.com/FuelLabs/fuel-c$ ore/pull/2154): Added `TransactionType$ ` type in `fuel-client`
- [2321](https://github.com/Fsh: 32: uelLabs/fuel-coSyntax error: "(" unexpectedre/pull/2321): New 
metrics for the TxPool:
$     - The size of transactions in the txpool (`txpoosh: 32: l_tx_size`)
 Syntax error: "(" unexpected   - The time spe
nt by a transaction in th$ e txpool in seconds (`txpool_tx_time_in_t$ xpool_seconds`)
    - T$ he number of transactions in the t$ xpool (`txpool_number_of_transactions`)
 $    - The number of transactions pending verification besh: 36: fore entering the txpoSyntax error: "(" unexpectedol (`txpool_numbe
r_of_transactions_pending_ver$ ification`)
    - The number o$ f executable transactions in $ the txpool (`txpool_number_o$ f_executable_transactions`)
    - Th$ e time it took to select transactiosh: 40: ns for inclusion iSyntax error: "(" unexpectedn a block in microsec
onds (`txpool_sele$ ct_transactions_time_microseconds`)
    - $ The time it took to insert a transaction in$  the txpool in microsecond$ s (`transaction_insertion_time_in_threa$ d_pool_microseconds`)
- [2385](htt$ ps://github.com/FuelLabs/fuel-core/pulsh: 45: l/2385): Added new histoSyntax error: "(" unexpectedgram buckets
 for some of the$  TxPool metrics, optimize the way they are$  collected.
- [2347](https://github$ .com/FuelLabs/fuel-core/pul$ l/2364): Add activity co$ ncept in order to protect against insh: 49: finitely increasingSyntax error: "(" unexpected DA gas price 
scenarios
- $ [2362](https://github.com/FuelLabs/fuel-core/pull/2sh: 49: 362): Added a new rSyntax error: "(" unexpectedequest_re
sponse protocol$  version `/fuel/req_res/0$ .0.2`. In comparison with$  `/fuel/req/0.0.1`, which returns sh: 51: an empty respSyntax error: "(" unexpectedonse when a
 request can$ not be fulfilled, this version returns more mean$ ingful error codes. Nodes still support the version `$ 0.0.1` of the protocol to guarantee backward compatibish: 53: lity with fuel-core nodes. Empty responses received from nodes using tSyntax error: "(" unexpectedhe old protocol
 `/fuel/req/0.0.1` are a$ utomatically converted into an error `ProtocolV1Emptsh: 53: yResponse` with error Syntax error: "(" unexpectedcode 0, which
 is also the only error$  code implemented. More specific error codes sh: 53: will be added iSyntax error: "(" unexpectedn the future.

- [2386](https://github$ .com/FuelLabs/fuel-core/pull/2386): Addsh: 53:  a flag to defineSyntax error: "(" unexpected the maximum num
ber of file descriptor$ s that RocksDB can use. By default it's halsh: 53: f of the OS limit.Syntax error: "(" unexpected
- [2376](htt
ps://github.com/Fuel$ Labs/fuel-core/pull/2376): Add $ a way to fetch transactions in P2P without$  specifying a peer.
- [2361](https$ ://github.com/FuelLabs/fuel-core/pull/236$ 1): Add caches to the sync service to not reask fosh: 57: r data it already fetSyntax error: "(" unexpectedched from the 
network.
- [2327](htt$ ps://github.com/FuelLabs/fush: 57: el-core/pull/2327): Syntax error: "(" unexpectedAdd more serv
ices tests and more che$ cks of the pool. Also add an high level dosh: 57: cumentation for users Syntax error: "(" unexpectedof the pool an
d contributors.
- [2$ 416](https://github.com/FuelLabs/fuel-core/issush: 57: es/2416): Define the `GasPriceSyntax error: "(" unexpectedServiceV1` task
.
- [2447](https://github.com$ /FuelLabs/fuel-core/pull/2447): Use new `expiration` sh: 57: policy in the transactSyntax error: "(" unexpectedion pool. Add a me
chanism to prune the transac$ tions when they expired.
- [1922](https://githush: 57: b.com/FuelLabs/fuel-Syntax error: "(" unexpectedcore/pull/1922): Ad
ded support for postin$ g blocks to the shared sequencer.
- [2033](httpsh: 57: s://github.com/FueSyntax error: "(" unexpectedlLabs/fuel-core/pu
ll/2033): Remove `Option<$ BlockHeight>` in favor of `BlockHeightQuery` where applicabsh: 57: le.
- [2490](httpSyntax error: "(" unexpecteds://github.com/Fuel
Labs/fuel-core/pull/2490): $ Added pagination support for the `balances` GraphQL query, availablsh: 57: e only when 'balancesSyntax error: "(" unexpected indexation' is en
abled.
- [2439](https://github.com$ /FuelLabs/fuel-core/pull/2439): Add gas costs for the two new zk osh: 57: pcodes `ecop` and `Syntax error: "(" unexpectedeadd` and the ben
ches that allow to cali$ brate them.
- [2472](https://github.com/FuelLabs/fuel-core/pull/247sh: 57: 2): Added the `amouSyntax error: "(" unexpectedntU128` field t
o the `Balance` GraphQL sche$ ma, providing the total balance as a `U128`. The existinsh: 57: g `amount` field Syntax error: "(" unexpectedclamps any balanc
e exceeding `U64` to `u64$ ::MAX`.
- [2526](https://github.com/FuelLabs/fuel-core/pull/2526): Adsh: 57: d possibility to not have aSyntax error: "(" unexpectedny cache
 set for RocksDB. Add an o$ ption to either load the RocksDB columns families on creation osh: 57: f the database orSyntax error: "(" unexpected when the column 
is used.
- [2532](h$ ttps://github.com/FuelLabs/fuel-core/pull/2532): Gesh: 57: tters for inner rocksdb Syntax error: "(" unexpecteddatabase hand
les.
- [2524](https://gi$ thub.com/FuelLabs/fuel-core/pull/2524): Adds a new lock tsh: 57: ype which is optimiSyntax error: "(" unexpectedzed for certai
n workloads to the t$ xpool and p2p services.
- [2535](https://github.com/FuelLabs/fuel-core/pull/2sh: 57: 535): Expose `backSyntax error: "(" unexpectedup` and `restore` 
APIs on the `Combin$ edDatabase` struct to create portable backups and rsh: 57: estore from them.Syntax error: "(" unexpected
- [2550](htt
ps://github.com/FuelLa$ bs/fuel-core/pull/2550): Add statistics and more limsh: 57: its infos about txpool on tSyntax error: "(" unexpectedhe node_info 
endpoint

### Fixed
- $ [2560](https://github.com/FuelLabs/fuel-core/pullsh: 57: /2560): Fix flaky teSyntax error: "(" unexpectedst by increasing t
imeout
- [2558](https:$ //github.com/FuelLabs/fuel-core/pull/2558): Renamesh: 57:  `cost` and `rewaSyntax error: "(" unexpectedrd` to remove `ex
cess` wording
- [2469](http$ s://github.com/FuelLabs/fuel-core/pull/2469): Ish: 57: mproved the logic foSyntax error: "(" unexpectedr syncing the ga
s price database with $ on_chain database
- [2365](https://github.com/FuelLsh: 57: abs/fuel-core/pullSyntax error: "(" unexpected/2365): Fixed
 the error during dry run$  in the case of race condition.
- [2366](sh: 57: https://github.comSyntax error: "(" unexpected/FuelLabs/fuel-co
re/pull/2366): The `i$ mporter_gas_price_for_block` metric is properly sh: 57: collected.
- [23Syntax error: "(" unexpected69](https://g
ithub.com/FuelLabs/fuel-core$ /pull/2369): The `transaction_insertion_time_in_thresh: 57: ad_pool_milliseconSyntax error: "(" unexpectedds` metric 
is properly collected.
- $ [2413](https://github.com/FuelLabs/fuel-core/issuessh: 57: /2413): block producSyntax error: "(" unexpectedtion immedia
tely errors if unable to $ lock the mutex.
- [2389](https://github.com/FuelLabs/sh: 57: fuel-core/pull/2Syntax error: "(" unexpected389): Fix construc
tion of reverse ite$ rator in RocksDB.
- [2479](https://github.com/FuelLabs/fush: 57: el-core/pull/2479): FiSyntax error: "(" unexpectedx an error on
 the last iteration of th$ e read and write sequential opcodes on contracsh: 57: t storage.
- [247Syntax error: "(" unexpected8](https://git
hub.com/FuelLabs/fue$ l-core/pull/2478): Fix proof created by `message_recesh: 57: ipts_proof` functiSyntax error: "(" unexpectedon by ignori
ng the receipts from fail$ ed transactions to match `message_outbox_root`.
-sh: 57:  [2485](https://gSyntax error: "(" unexpectedithub.com/FuelLabs/f
uel-core/pull/2485): $ Hardcode the timestamp of the genesis block and verssh: 57: ion of `tai64` to aSyntax error: "(" unexpectedvoid breaking
 changes for us.
- [2$ 511](https:/$ /github.com/FuelLabs/fuel-core/pull/2511): $ Fix backward compatibility of V0Metadata in gassh: 59:  price db.

###Syntax error: "(" unexpected Changed
- [246
9](https://github.com/Fue$ lLabs/fuel-core/pull/2469): Updated adapter for qush: 59: erying costs fromSyntax error: "(" unexpected DA Block committe
r API
- [2469](https$ ://github.com/FuelLabs/fuel-core/pull/2469): Use the gash: 59: s price from the lSyntax error: "(" unexpectedatest block to est
imate future gas prices
$ - [2501](https://github.com/FuelLabs/fuel-core/sh: 59: pull/2501): Use gas Syntax error: "(" unexpectedprice from blo
ck for estimating future $ gas prices
- [2468](https://github.com/FuelLabs/fuel-sh: 59: core/pull/2468): AbstSyntax error: "(" unexpectedract unrecor
ded blocks concept f$ or V1 algorithm, create new storage impl. Introduce `Transh: 59: sactionableStorage` trait to allowSyntax error: "(" unexpected atomic changes 
to the storage.
- [$ 2295](https://github.com/FuelLabs/fuel-core/pull/2295)sh: 59: : `CombinedDb::frSyntax error: "(" unexpectedom_config` now res
pects `state_rewind_policy$ ` with tmp RocksDB.
- [2378](https://github.csh: 59: om/FuelLabs/fuelSyntax error: "(" unexpected-core/pull/2378
): Use cached hash of$  the topic instead of calculating it on each publishish: 59: ng gossip message.
- [243Syntax error: "(" unexpected8](https://githu
b.com/FuelLabs/fue$ l-core/pull/2438): Refactored service to use new implementatish: 59: on of `StorageReSyntax error: "(" unexpectedad::read` tha
t takes an offset in inp$ ut.
- [2429](https://github.com/FuelLabs/fuel-core/push: 59: ll/2429): IntroduSyntax error: "(" unexpectedce custom enum f
or representing re$ sult of running service tasks
- [2377](https://githsh: 59: ub.com/FuelLabs/fuel-core/Syntax error: "(" unexpectedpull/2377): Ad
d more errors that can b$ e returned as responses when using protocol `$ /fuel/req_res/0.0.2`. The errors supporte$ d are `ProtocolV1EmptyResponse` (status code `0`sh: 61: ) for converting emptSyntax error: "(" unexpectedy responses s
ent via protocol `/fue$ l/req_res/0.0.1`, `RequestedRangeTooLarge`(status codesh: 61:  `1`) if the cliSyntax error: "(" unexpectedent requests a
 range of objects suc$ h as sealed block headers or transactions too large,sh: 61:  `Timeout` (status Syntax error: "(" unexpectedcode `2`) if the 
remote peer takes too lo$ ng to fulfill a request, or `SyncProcessorOutOfCapacity` ish: 61: f the remote peer is fulfilSyntax error: "(" unexpectedling too many
 requests concurrently.$ 
- [2233](https://github.com/FuelLabs/fuel-core/psh: 61: ull/2233): Introduce a new column `modification_history_v2` for storing the mSyntax error: "(" unexpectedodification hi
story in the historical rocksD$ B. Keys in this column are stored in big endian order. Chash: 61: nged the behaviour Syntax error: "(" unexpectedof the historical
 rocksDB to write cha$ nges for new block heights to the new column, and to persh: 61: form lookup of vaSyntax error: "(" unexpectedlues from the `mod
ification_history_v2`$  table first, and then from the `modification_sh: 61: history` table, pSyntax error: "(" unexpectederforming a migrat
ion upon access if necessa$ ry.
- [2383](https://github.com/FuelLabs/fuel-core/pull/2383): The `balancsh: 61: e` and `balances` Syntax error: "(" unexpectedGraphQL query han
dlers now use index to p$ rovide the response in a more performant way. As the indexsh: 61:  is not created retroactSyntax error: "(" unexpectedively, the clie
nt must be initialized with$  an empty database and synced from the genesis block to utilize it. Otherwise, the legacy way of retrieving data will be used.
- [2463](https://github.com/FuelLabs/fuel-core/pull/2463): The `coinsToSpend` GraphQL query handler now uses index to provide the response in a more performant way. As the index is not created retroactively, the client must be initialized with an empty database and synced from the genesis block to utilize it. Otherwise, the legacy way of retrieving data will be used.
- [2556](https://github.com/FuelLabs/fuel-core/pull/2556): Ensure that the `last_recorded_height` is set for the DA gas price source.

#### Breaking
- [2469](https://github.com/FuelLabs/fuel-core/pull/2469): Move from `GasPriceServicev0` to `GasPriceServiceV1`. Include new config values.
- [2438](https://github.com/FuelLabs/fuel-core/pull/2438): The `fuel-core-client` can only work with new version of the `fuel-core`. The `0.40` and all older versions are not supported.
- [2438](https://github.com/FuelLabs/fuel-core/pull/2438): Updated `fuel-vm` to `0.59.1` release. Check [release notes](https://github.com/FuelLabs/fuel-vm/releases/tag/v0.59.0) for more details.
- [2389](https://github.com/FuelLabs/fuel-core/pull/2258): Updated the `messageProof` GraphQL schema to return a non-nullable `MessageProof`.
- [2154](https://github.com/FuelLabs/fuel-core/pull/2154): Transaction graphql endpoints use `TransactionType` instead of `fuel_tx::Transaction`.
- [2446](https://github.com/FuelLabs/fuel-core/pull/2446): Use graphiql instead of graphql-playground due to known vulnerability and stale development.
- [2379](https://github.com/FuelLabs/fuel-core/issues/2379): Change `kv_store::Value` to be `Arc<[u8]>` instead of `Arc<Vec<u8>>`.
- [2490](https://github.com/FuelLabs/fuel-core/pull/2490): Updated GraphQL complexity calculation for `balances` query to account for pagination (`first`/`last`) and nested field complexity (`child_complexity`). Queries with large pagination values or deeply nested fields may have higher complexity costs.
- [2463](https://github.com/FuelLabs/fuel-core/pull/2463): 'CoinsQueryError::MaxCoinsReached` variant has been removed. The `InsufficientCoins` variant has been renamed to `InsufficientCoinsForTheMax` and it now contains the additional `max` field
- [2463](https://github.com/FuelLabs/fuel-core/pull/2463): The number of excluded ids in the `coinsToSpend` GraphQL query is now limited to the maximum number of inputs allowed in transaction.
- [2463](https://github.com/FuelLabs/fuel-core/pull/2463): The `coinsToSpend` GraphQL query may now return different coins, depending whether the indexation is enabled or not. However, regardless of the differences, the returned coins will accurately reflect the current state of the database within the context of the query.
- [2526](https://github.com/FuelLabs/fuel-core/pull/2526): By default the cache of RocksDB is now disabled instead of being `1024 * 1024 * 1024`.

## [Version 0.40.2]

### Fixed

- [2476](https://github.com/FuelLabs/fuel-core/pull/2476): Hardcode the timestamp of the genesis block.

## [Version 0.40.1]

### Added

- [2450](https://github.com/FuelLabs/fuel-core/pull/2450): Added support for posting blocks to the shared sequencer.

## [Version 0.40.0]

### Added
- [2347](https://github.com/FuelLabs/fuel-core/pull/2347): Add GraphQL complexity histogram to metrics.
- [2350](https://github.com/FuelLabs/fuel-core/pull/2350): Added a new CLI flag `graphql-number-of-threads` to limit the number of threads used by the GraphQL service. The default value is `2`, `0` enables the old behavior.
- [2335](https://github.com/FuelLabs/fuel-core/pull/2335): Added CLI arguments for configuring GraphQL query costs.

### Fixed
- [2345](https://github.com/FuelLabs/fuel-core/pull/2345): In PoA increase priority of block creation timer trigger sh: 61: compare to txpSyntax error: "(" unexpectedool event man
agement

### Changed
- [23$ 34](https://github.com/FuelLabs/fuel-core/pull/2334): Prepare sh: 61: the GraphQL servicSyntax error: "(" unexpectede for the swit
ching to `async` method$ s.
- [2310](https://github.com/FuelLabs/fuel-core/push: 61: ll/2310): New metrics: "ThSyntax error: "(" unexpectede gas prices us
ed in a block" (`importer_ga$ s_price_for_block`), "The total gas used$  in a block" (`importer_gas_per_block`), "$ The total fee (gwei) paid by transactions in a blocsh: 63: k" (`importer_fee_pSyntax error: "(" unexpecteder_block_gwei
`), "The total nu$ mber of transactions in a block" (`importer_transactsh: 63: ions_per_block`Syntax error: "(" unexpected), P2P metrics 
for swarm and protocol$ .
- [2340](https://github.com/FuelLabs/fuel-core/pulsh: 63: l/2340): Avoid loSyntax error: "(" unexpectedng heavy tasks
 in the GraphQ$ L service by splitting work into bash: 63: tches.
- [23Syntax error: "(" unexpected41](https://gith
ub.com/FuelLabs/fuel$ -core/pull/2341): Updated all pagination queries to wosh: 63: rk with the async stSyntax error: "(" unexpectedream instead of the
 sync iterator.
- [2$ 350](https://github.cosh: 63: m/FuelLabs/fuel-coSyntax error: "(" unexpectedre/pull/2350): Li
mited the number of threa$ ds used by the GraphQL service.

#### Breaking
sh: 63: - [2310](https://Syntax error: "(" unexpectedgithub.com/Fu
elLabs/fuel-core/pull/2$ 310): The `metrics` command-line parameter has been replacedsh: 63:  with `disable-metricSyntax error: "(" unexpecteds`. Metrics a
re now enabled by de$ fault, with the option to disable them entirely or on a sh: 63: per-module basis.
Syntax error: "(" unexpected- [2341](https:
/$ /github.com/FuelLabs/fuel-core/pull/2341): The maximum numbersh: 63:  of processed Syntax error: "(" unexpectedcoins from
 the `coins_to_s$ pend` query is limited to `max_inputs`.
sh: 63: 
### FixeSyntax error: "(" unexpectedd

- [
2352](https:/$ /github.com/FuelLabs/fuel-core/sh: 63: pull/2352): Syntax error: "(" unexpectedCache p2p 
responses to se$ rve without roundtrip to $ db.

## [Version 0.39.0$ ]

### Added
- [2324]$ (https://github.com/FuelLabs$ /fuel-core/pull/2324): Ad$ ded metrics for sync, async prsh: 68: ocessor andSyntax error: "(" unexpected for all G
raphQL queries.$ 
- [2320](htt$ ps://github.com/FuelLabs/fuel-core/pull/2320):$  Added new CLI flag `graphql$ -max-resolver-recursive-depth` to limit recursio$ n within resolver. The default value $ it "1".

## Fixed
- [2320](https://github.com/sh: 73: FuelLabs/fuel-coSyntax error: "(" unexpectedre/issues/23
20): Prevent `/health` and `$ /v1/health` from being throttled by th$ e concurrency limiter.
- [2322](https$ ://github.com/FuelLabs/fuel-core/iss$ ues/2322): Set the salt of genesis cont$ racts to zero on execution.
- [2324](https://gitsh: 77: hub.com/FuelLabs/Syntax error: "(" unexpectedfuel-core/pull/232
4): Ignore peer i$ f we already are syncing transactions from it.

#### Bsh: 77: reaking

- [232Syntax error: "(" unexpected0](https://github.
com/FuelLabs/fuel-core/pull/$ 2330): Reject queries that are recursive during sh: 77: the resolution of the Syntax error: "(" unexpectedquery.

### Chang
ed

#### Breaking
$ - [2311](https://github.com/FuelLabs/fuel-c$ ore/pull/2311): Changed the text of the $ error returned by the executor if gas overflows.
sh: 79: 
## [Version 0.38.0]

Syntax error: "(" unexpected### Added

- [2309](https://git$ hub.com/FuelLabs/fuel-core/pull/230$ 9): Limit number of concurrent queries t$ o the graphql service.
- [2216](https://github.sh: 81: com/FuelLabs/fuel-cSyntax error: "(" unexpectedore/pull/2216): A
dd more function to t$ he state and task of TxPoolV2 to handle the future interactions sh: 81: with others modules Syntax error: "(" unexpected(PoA, BlockProdu
cer, BlockImporter $ and P2P).
- [2263](https://github.com/FuelLabs/fuel-core/sh: 81: pull/2263): TransSyntax error: "(" unexpectedaction pool is now
 included in all modules of$  the code it has requires modifications on different modush: 81: les :
    - The PoA iSyntax error: "(" unexpecteds now notify o
nly when there is new tra$ nsaction and not using the `tx_update_sender` ash: 81: nymore.
    - TheSyntax error: "(" unexpected Pool transactio
n source for the execu$ tor is now locking the pool until the blo$ ck production is finished.
    - Read$ ing operations on thsh: 83: e pool is now asyncSyntax error: "(" unexpectedhronous and it’s 
the less prioritized opera$ tion on the Pool, API has been updated accordingly.
    sh: 83: - GasPrice is no morSyntax error: "(" unexpectede using async 
to allow the transaction$ s verifications to not use asy$ nc anymore

    We also added a lot of ne$ w configuration cli parameters to fine-$ tune TxPool configuration.
    This PR also changsh: 86: es the way we are maSyntax error: "(" unexpectedking the heavy
 work processor an$ d a sync a$ nd asynchronous version is available in servi$ ces folder (usable by anyone)
    P2P now use sep$ arate heavy work processor for DB and TxPo$ ol interactions.

### Removed
- [2306](https:sh: 90: //github.com/FuelLaSyntax error: "(" unexpectedbs/fuel-core/pull/
2306): Removed hack for g$ enesis asset contract from the code.

## [Versiosh: 90: n 0.37.1]

### Syntax error: "(" unexpectedFixed
- [2304]
(https://github.com/Fue$ lLabs/fuel-core/pull/2304): Add initialization $ for the genesis base asset contract.
$ 
### Added
- [2288](https://github.com/Fush: 92: elLabs/fuel-core/pull/22Syntax error: "(" unexpected88): Specify `V
1Metadata` for `GasPrice$ ServiceV1`.

## [Version 0.37.0]

### Adsh: 92: ded
- [1609](httpSyntax error: "(" unexpecteds://github.
com/FuelLabs/fuel-cor$ e/pull/1609): Add DA compression support. Compressed bsh: 92: locks are stored in tSyntax error: "(" unexpectedhe offchain datab
ase when blocks are pro$ duced, and can be fetched using the Grap$ hQL API.
- [2290](https://github.com/F$ uelLabs/fuel-core/pull/2290): Added a new CLI a$ rgument `--graphql-max-directives`. The default valush: 95: e is `10`.
- [2195]Syntax error: "(" unexpected(https://github.c
om/FuelLabs/fuel-core/pul$ l/2195): Added enforcement of the limit on$  the size of the L2 transactions per block $ according to the `block_transaction_size_li$ mit` parameter.
- [2131](https://githu$ b.com/FuelLabs/fuel-core/pull/2131): Add flow insh: 99:  TxPool in order to Syntax error: "(" unexpectedask to ne
wly connected pe$ ers to share their transaction pool
$ - [2182](https://github.com/FuelLabs/fue$ l-core/pull/2151): Limit n$ $ umber of transactions that can sh: 103: be fetchedSyntax error: "(" unexpected via TxSource::nex
t
- [2189](https://git$ hub.com/FuelLabs/fuel-core/pull/2151): Select next DA heightsh: 103:  to never include more Syntax error: "(" unexpectedt
$ han u16::MAX -1 transactions from L1.
- [2265](httpssh: 103: ://github.com/FuelLabs/Syntax error: "(" unexpectedfuel-core/pull/
2265): Integrate Block C$ ommitter API for DA Block Costs.
- [2162](https://github.com/FuelLabs/fuel-core/pull/2162): Pool structure with dependencies, etc.. for the next transaction pool module. Also adds insertion/verification process in PoolV2 and tests refactoring
- [2280](https://github.com/FuelLabs/fuel-core/pull/2280): Allow comma separated relayer addresses in cli
- [2299](https://github.com/FuelLabs/fuel-core/pull/2299): Support blobs in the predicates.
- [2300](https://github.com/FuelLabs/fuel-core/pull/2300): Added new function to `fuel-core-client` for checking whether a blob exists.

### Changed

#### Breaking
- [2299](https://github.com/FuelLabs/fuel-core/pull/2299): Anyone who wants to participate in the transaction broadcasting via p2p must upgrade to support new predicates on the TxPool level.
- [2299](https://github.com/FuelLabs/fuel-core/pull/2299): Upgraded `fuel-vm` to `0.58.0`. More information in the [release](https://github.com/FuelLabs/fuel-vm/releases/tag/v0.58.0).
- [2276](https://github.com/FuelLabs/fush: 1: el-core/pull/2276): Changed how complexity for blocks is catx_update_sender: not foundlculated. Th
e default complexity now is 80_000. All queries that somehow touch the block header now are more expensive.
- [2290](https://github.com/FuelLabs/fuel-core/pull/2290): Added a new GraphQL limit on number of `directives`. The default value is `10`.
- [2206](https://github.com/FuelLabs/fuel-core/pull/2206): Use timestamp of last block when dry running transactions.
- [2153](https://github.com/FuelLabs/fuel-core/pull/2153): Updated default gas costs for the local testnet configuration to match `fuel-core 0.35.0`.

## [Version 0.36.0]

### Added
- [2135](https://github.com/FuelLabs/fuel-core/pull/2135): Added metrics logging for number of blocks served over the p2p req/res protocol.
- [2151](https://github.com/FuelLabs/fuel-core/pull/2151): Added limitations on gas used during dry_run in API.
- [2188](https://github.com/FuelLabs/fuel-core/push: 103: -: not found
$ ll/2188): Added the new variant `V2` for the `ConsensusParameters` which contains the new `block_transaction_size_lsh: 104: -: not found
$ imit` parameter.
- [2163](https://github.com/FuelLabs/fuel-core/pull/2163): Added runnable task for fetching block committer data.
- [2204](https:sh: 105: -: not found
$ //github.com/FuelLabs/fuel-core/pull/2204): Added `dnsaddr` resolution for TLD without suffixes.

### Csh: 106: -: not found
$ h$ anged

#### Breaking
- [2199](https://github.com/FuelLabs/fuel-core/pull/2199): Applying severash: 108: We: not found
$ l breaking changes to the WASM interface from backlsh: 109: og:
  - Get theSyntax error: "(" unexpected module to ex
ecute WASM byte code f$ rom the storage first, an fallback to the built-in version in the case of the `FUEL_ALWAYS_USE_WASM`.
  - Added `host_v1` with a new `pesh: 109: P2P: not found
$ e$ $ k_next_txs_size` mesh: 112: thod, that acceptsSyntax error: "(" unexpected `tx_number_
limit` and `size_limi$ t`.
  - Added new variant of th$ e return type to pass the$  validation result. I$ t removes b$ lock serialization and deseriash: 116: lization and shouSyntax error: "(" unexpectedld improve performance.

  - Added a V1 execution $ result type that us$ es `JSONErr$ or` instead of postcard sersh: 118: ialized error. ISyntax error: "(" unexpectedt adds flexib
ility of how varian$ ts of the error can be managed.$  Mo$ re information ab$ out it in h$ ttps://github.com/FuelLash: 122: bs/fuel-vm/issueSyntax error: "(" unexpecteds/797. The c
hange also moves `T$ ooManyOutputs` error to the top. It shows thash: 122: t `JSONError` worSyntax error: "(" unexpectedks as expect
ed.
- [2145](htt$ ps://github.com/FuelLabs/fuel-core/pull/2145): fsh: 122: eat: Introduce tSyntax error: "(" unexpectedime port in 
PoA service.
- [21$ 55](https://github.com/FuelLabs/fuel-core/push: 122: ll/2155): Added Syntax error: "(" unexpectedtrait declar
ation for block co$ mmitter data
- [2142](https://github.com/sh: 122: FuelLabs/fuel-coSyntax error: "(" unexpectedre/pull/2142):
 Added benchmarks $ for varied forms of db lookups to assist in opsh: 122: timizations.
-Syntax error: "(" unexpected [2158](https
://github.com/FuelL$ abs/fuel-core/push: 122: ll/2158): Log tSyntax error: "(" unexpectedhe public add
ress of the signing$  key, if it is specified
- [2188](https://githush: 122: b.com/FuelLabs/fuSyntax error: "(" unexpectedel-core/pull/2
188): Upgraded the `$ fuel-vm` to `0.57.0`. More information in sh: 122: the [release](htSyntax error: "(" unexpectedtps://github.
com/FuelLabs/fuel-v$ m/releases/tag/v0.57.0).

## [Version 0sh: 122: .35.0]

### AdSyntax error: "(" unexpectedded
- [2122
](https://github.com$ /FuelLabs/fuel-core/pull/2122): Changed the resh: 122: layer URI addressSyntax error: "(" unexpected to be a vect
or and use a quorum $ provider. Th$ e `relayer` argument now supports$  multiple URLs to fetch informatio$ n from different sources.
- [2119$ ](https://github.com/FuelLabs/fuel-core/pull/21sh: 126: 19): GraphQL querSyntax error: "(" unexpectedy fields for 
retrieving informati$ on about upgrades.

### Changed
- [2113](htsh: 126: tps://github.coSyntax error: "(" unexpectedm/FuelLabs/fu
el-core/pull/2113):$  Modify the way the gas price service and shared ash: 126: lgo is initialiSyntax error: "(" unexpectedzed to have s
ome default value b$ ased on best guess instead of `None`, and initsh: 126: ialize service Syntax error: "(" unexpectedbefore graphq
l.
- [2112](htt$ ps://github.com/FuelLabs/fuel-core/pull/2112sh: 126: ): Alter the waySyntax error: "(" unexpected the sealed 
blocks are fetched $ with a given height.
- [2120](https://githush: 126: b.com/FuelLabs/Syntax error: "(" unexpectedfuel-core/pu
ll/2120): Add$ ed `submitAndAwaitStatus` subscrip$ tion endpoint which returns the `$ SubmittedStatus` after the tran$ saction is submitted as well as $ the `TransactionStatus` subscription.
- [sh: 130: 2115](https://gSyntax error: "(" unexpectedithub.com/Fu
elLabs/fuel-core/p$ ull/2115): Add test for `SignMode` `is_ash: 130: vailable` methSyntax error: "(" unexpectedod.
- [212
4](https://github$ .com/FuelLabs/fuel-core/pull/2124): Generalish: 130: ze the way p2p Syntax error: "(" unexpectedreq/res prot
ocol handles requ$ ests.

#### Breaking

- [2040](httpsh: 130: s://github.comSyntax error: "(" unexpected/FuelLabs/fu
el-core/pull/2040)$ : Added full `no_std` support state transh: 130: sition relatedSyntax error: "(" unexpected crates. The
 crates now requ$ ire the "alloc" feature to be e$ nabled. Following crates are aff$ ected:
  - `fuel-core-types`
$   - `fuel-core-storage`
  - `fuel-core-executor`
- [2116](https://github.com/FuelLabs/fuel-core/pull/2116): Replace `H160` in config and cli options of relayer by `Bytes20` of `fuel-types`

### $ Fixed
- [2134](https://github.com/FuelLabs/sh: 134: fuel-core/pull/Syntax error: "(" unexpected2134): Perfor
m RecoveryID normal$ ization for AWS KMS -generated signatures.

## [Version 0.34.0]

### Added
- [2051](https://github.com/FuelLabs/fuel-core/pull/2051): Add support for AWS KMS signing for the PoA consensus module. The new key can be specified with `--consensus-aws-kms AWS_KEY_ARN`.
- [2092](https://github.com/FuelLabs/fuel-core/pull/2092): Allow iterating by keys in rocksdb, and other storages.
- [2096](https://github.com/FuelLabs/fuel-core/pull/2096): GraphQL query field to fetch blob byte code by its blob ID.

### Changed
- [2106](https://github.com/FuelLabs/fuel-core/pull/2106): Remove deadline clock in POA and replace with tokio time functions.

- [2035](https://github.com/FuelLabs/fuel-core/pull/2035): Small code optimizations.
    - The optimized code specifies the capacity when initializing the HashSet, avoiding potential multiple reallocations of memory durinsh: 1: g element insertion.
FUEL_ALWAYS_USE_WASM: not found    - The opt
imized code uses the return value of HashSet::insert to check if the insertion was successful. If the insertion fails (i.e., the element already exists), it returns an error. This reduces one lookup operatish: 134: -: not found
$ on.
    - The optimized code simplifies the initialization logic of exclude by using the Option::map_or_else sh: 1: host_v1: not found
sh: 1: peek_next_txs_size: not found
sh: 1: tx_number_limit: not found
sh: 1: size_limit: not found
sh: 135: -: not found
$ method.

#### Breaking
- [2051](https://github.com/FuelLabs/fuel-core/pull/2051): Misdocumented `CONSENSUS_KEY` environ variable has been removed, use `CONsh: 136: -: not found
$ SENSUS_KEY_SECRET` instead. Also raises MSRV to `1.79.0`.

### Fixed

- [2106](https://github.com/FuelLabs/fuel-core/pull/2106): Handle the case when nodes with overriding start on the fresh network.
- [2105](https://github.com/FuelLabs/fuel-core/pull/2105): Fixed the rollback functionality to work with empty gas price database.

## [Versish: 1: JSONError: not found
sh: 1: TooManyOutputs: not found
sh: 1: JSONError: not found
sh: 137: -: not found
$ on 0.33.0]

### Added
- [20sh: 138: 94](https://github.cSyntax error: "(" unexpectedom/FuelLabs/f
uel-core/pull/2094): Added suppo$ rt for presh: 138: defined blocks Syntax error: "(" unexpectedprovided via t
he filesystem.
- [20$ 94](https://github.com/FuelLabs/fuel-core/pull/2094):sh: 138:  AddedSyntax error: "(" unexpected `--predefine
d-blocks-path` CLI argu$ ment to pass the path to the predefined blocks.
sh: 138: - [2081]Syntax error: "(" unexpected(https://git
hub.com/FuelLabs/fuel$ -core/pull/2081): Enable producer to include predefinedsh: 138:  blocks.
- [2079Syntax error: "(" unexpected](https://gi
thub.com/FuelLabs/fue$ l-core/pull/2079): Open un$ known columns in the RocksDB for $ forward compatibility.

### Change$ d
- [2076](https://github.com/FuelLa$ bs/fuel-core/pull/2076): Replace usages of `iter_all` with `sh: 142: iter_all_keys` wheSyntax error: "(" unexpectedre necessa
ry.

#### Breaking
-$  [2080](https://github.com/FuelLabs/fuel-core/push: 142: ll/2080): Reject Syntax error: "(" unexpectedUpgrade txs w
ith invalid wasm on txpoo$ l level.
- [2082](https://github.co$ m/FuelLabs/fuel-core/pull/2088): M$ ove `TxPoolError` from `fuel-core-types` to `fuel-cosh: 144: re-txpool`.
-Syntax error: "(" unexpected [2086](https:/
/github.com/FuelLabs$ /fuel-core/pull/2086): Added support for Posh: 144: A key rotation.
- Syntax error: "(" unexpected[2086](https:/
/github.com/FuelLabs/f$ uel-core/pull/2086): Support overriding of the non consh: 144: sensus parameteSyntax error: "(" unexpectedrs in the chain
 config.

### Fixed
$ 
- [2094](https://github.com/FuelLabs/fuel-core/sh: 144: pull/2094): FixedSyntax error: "(" unexpected bug in rollb
ack logic because of w$ rong ordering of modifications.
sh: 144: 
## [Version 0.32Syntax error: "(" unexpected.1]

###
 Added
- [2061](htt$ ps://github.com/FuelLabs/fuel-core$ /pull/2061): Allow querying filled tr$ ansaction body from the statu$ s.

### Changed
- [2067](https://github.com/FuelLabssh: 147: /fuel-core/pull/2067): Syntax error: "(" unexpectedReturn error fr
om TxPool level if the `$ BlobId` is known.
- [2064](https://github.com/FuelLabs/fuel-core/pull/2064): Allow gas price metadata values to be overridden with config

### Fixes
- [2060](https://github.com/FuelLabs/fuel-core/pull/2060): Use `min-gas-price` as a starting point if `start-gas-price` is zero.
- [2059](https://github.com/FuelLabs/fuel-core/pull/2059): Remove unwrap that is breaking backwards compatibility
- [2063](https://github.com/FuelLabs/fuel-core/pull/2063): Don't use historical view during dry run.

## [Version 0.32.0]

### Added
- [1983](https://github.com/FuelLabs/fuelsh: 1: fuel-core-types: not found
sh: 147: -: not found
$ -core/pull/1983): Add adsh: 1: fuel-core-storage: not found
sh: 148: -: not found
$ apters for gas price servsh: 1: fuel-core-executor: not found
sh: 149: -: not found
$ ice for accessing database vsh: 150: alues

### BrSyntax error: "(" unexpectedeaking
- [2
048](https://github.com/$ FuelLabs/fuel-core/pull/2048): Dis$ able SMT for `ContractsAssets`$  and `Contrsh: 152: actsState` for tSyntax error: "(" unexpectedhe productio
n mode of the `fue$ l-core`. The SMT still is used in b$ enchmarks and tests.
- [#1988](https$ ://github.$ com/FuelLabs/fuel-core$ /pull/1988sh: 156: ): Updated `fuelSyntax error: "(" unexpected-vm` to `0.56.
0` ([release notes]($ https://github.com/FuelLabs/fuel-vm/releassh: 156: es/tag/v0.55.0))Syntax error: "(" unexpected. Adds Blob tr
ansaction support.
$ - [2025](https://github.com/FuelLabs/fuel-csh: 156: ore/puSyntax error: "(" unexpectedll/2025): Ad
d new V0 algorith$ m for gas price to services.
 $    This change includes new flags f$ or the CLI:
        - "starting-gas-prish: 158: ce" - the startiSyntax error: "(" unexpectedng gas price 
for the gas price al$ gorithm
        - "gas-price-chang$ e-percent" - the psh: 159: ercent change forSyntax error: "(" unexpected each gas pr
ice update
        $ - "gas-price-threshold-percent" - the threshold percent for determining if the gas price will be increase or decreased
    And the following CLI flags are serving a new purpose
        - "min-gas-price" - the minimum sh: 159: gas price that the gas pric-: not founde algorithm w
ill return
- [2045](https:$ //github.com/FuelLabs/fuel-core/pull/2045): Include sh: 160: withdrawal messaSyntax error: "(" unexpectedge only if tr
ansaction is execute$ d successfully.
- [2041](https://github.com/FuelLabs/fuel-core/pull/2041): Add code for startup of the gas price algorithm updater so
    the gas price db on startup is always in sync with the onsh: 160:  chain db

## [-: not foundVersion 0.31.0
]

### Added
- [#201$ 4](https://github.com/FuelLabs/fuel-$ core/pu$ ll/2014): Addedsh: 163:  a separate thrSyntax error: "(" unexpectedead for the bl
ock importer.
- [#$ 2013](https://github.com/FuelLabs/f$ uel-core/pull/2013): Added a separa$ te thread to process P2P database $ lookups.
- [#2004](https://github.com/Fsh: 166: uelLabs/fuel-coreSyntax error: "(" unexpected/pull/2004):
 Added new CLI arg$ ument `continue-services-on-error` to control sh: 166: internal flow ofSyntax error: "(" unexpected services.

- [#2004](https://g$ ith$ ub.com/FuelLabs/fuel-core/pull/200$ 4): Added handling of incorrect$  shutdown of the off-chain GraphQL$  worker by using state rewind feash: 170: ture.
- [#2007]Syntax error: "(" unexpected(https://git
hub.com/FuelLabs/fue$ l-core/pull/2007): Improved metrics:
  - Adsh: 170: ded database meSyntax error: "(" unexpectedtrics per col
umn.
  - Added $ statistic about commitsh: 170:  time of each daSyntax error: "(" unexpectedtabase.
  - 
Refactored how metri$ cs are registered: Now, we use only one resh: 170: gister shared betSyntax error: "(" unexpectedween all metri
cs. This global re$ gister is used to encode all metr$ ics.
- [#1996](https://github.c$ om/FuelLabs/fuel-core/pull/1996): Added sush: 172: pport foSyntax error: "(" unexpected
r rollback command$  when state $ rewind feature is enabled. The comma$ nd allows the rollback of the state of thesh: 174:  blockchain seveSyntax error: "(" unexpectedral blocks beh
ind until the end o$ f the historical window. The default historsh: 174: ical window it 7 Syntax error: "(" unexpecteddays.
- [#19
96](https://github.$ com/FuelLabs/sh: 174: fuel-core/pull/Syntax error: "(" unexpected1996): Added
 support for the st$ ate rewind feature. The feature allows the exsh: 174: ecution of the bSyntax error: "(" unexpectedlocks in the 
past and the same e$ xecution results to be received. $ Together with forkless upgrades,$  execution of any block from the$  past is possible if historical data exist forsh: 177:  the target blocSyntax error: "(" unexpectedk height.

- [#1994](https://gi$ thub.com/FuelLabs/f$ uel-core/pull/1994): Added the act$ ual implementation for the `Atomic$ View::latest_view`.
- [#1972](h$ ttps://github.com/FuelLabs/fuel-core/pull/sh: 181: 1972): ImplementSyntax error: "(" unexpected `AlgorithmU
pdater` for `GasPri$ ceService`
- [#1948](https://git$ hub.com/FuelLabs/fuel-core/pull/1$ 948): Add new `Algoritsh: 183: hmV1` and `AlgoriSyntax error: "(" unexpectedthmUpdaterV1` f
or the gas price. Incl$ ude tools for analysis
- [#1676](https://sh: 183: github.com/FuelLSyntax error: "(" unexpectedabs/fuel-cor
e/pull/1676): Added$  new CLI arguments:
    - `graph$ ql-max-depth`
    - `graphql-max$ -complexity`
    - `graphql-max-recursive-dsh: 185: epth`

### ChSyntax error: "(" unexpectedanged
- [#2
015](https://$ github.com/FuelLabs/fuel-core/pull/2015): Ssh: 185: mall fixes for tSyntax error: "(" unexpectedhe database:

  - Fixed the name $ for historical columns - Metrics was worksh: 185: ing incorrectly Syntax error: "(" unexpectedfor historica
l columns.
  - A$ dded recommended setting for the $ RocksDB - The source of recommendat$ ion is official documen$ tation https://github.com/facebook/$ rocksdb/wiki/Setup-Options-and-Basic-Tuningsh: 189: #other-general-Syntax error: "(" unexpectedoptions.
  -
 Removed repairing si$ nce it could corrupt the database if$  fails - Several users reported abou$ t the corrupted stash: 191: te of the databSyntax error: "(" unexpectedase after ha
ving a "Too many des$ criptors" error where in logs, repairing of the dash: 191: tabase also failSyntax error: "(" unexpecteded with this
 error creating a `l$ ost` folder.
- [#2010](https://github.csh: 191: om/FuelLabs/fuelSyntax error: "(" unexpected-core/pull/20
10): Updated the b$ lock importer to allow more blocks to be in the queue. It improves synchronization speed and mitigate the impact of other services on synchronization speed.
- [#2006](https://github.com/FuelLabs/fuel-core/pull/2006): Process block importer events first under P2P pressure.
- [#2002](https://github.com/FuelLabs/fuel-core/pull/2002): Adapted the block producer to react to chesh: 191: cked transactionsThis: not found that were 
using another version of c$ onsensus parameters during validation in the TxPool. After an upgrade of the consensus parameters of the network, TxPool could storsh: 192: -: not found
$ e invalid `Checked` transactions. This change fixes that by tracking the version thash: 193: -: not found
$ t was used to validate the transactions.
- [#1999](https://github.com/FuelLabs/fuel-core/pull/1999): Minimize the number of pansh: 194: -: not found
$ ics in the codebase.
- [#1990](https://github.com/FuelLabssh: 195: And: not found
$ /fuel-core/pull/1990): Use latest view for mutate GraphQL queries after modification of thesh: 196: -: not found
$ sh: 197:  node.
- [#1Syntax error: "(" unexpected992](https://
github.com/FuelLabs/fu$ el-core/pull/1992): Parse multiple relayer csh: 197: ontracts, `RELAYESyntax error: "(" unexpectedR-V2-LISTENIN
G-CONTRACTS$ ` env variable using a `,` delimiter.
- [#1980](https://github.com/FuelLabs/fuel-core/pull/1980): Add `Transaction` to relayer 's event filter

#### Breaking
- [#2012](https://github.com/Fush: 197: the: not found
$ $ el$ Labs/fuel-core/pull$ /2012): Bum$ sh: 202: Syntax error: "(" unexpectedped the `
fuel-vm` to `$ 0.55.0` release. More about tsh: 202: he change [here](httSyntax error: "(" unexpectedps://github.
com/FuelLabs/fuel-vm/rele$ ases/tag/v0.55.0).
- [#2001](httsh: 202: ps://github.com/FSyntax error: "(" unexpecteduelLabs/fuel-cor
e/pull/2001): Prevent$  GraphQL query body to be huge sh: 202: and cause OOM. TheSyntax error: "(" unexpected default body size
 is `1MB`. The limit$  can be changed by the `graphql-request-body-bytesh: 202: s-limit` CLI argument.Syntax error: "(" unexpected
- [#1991](ht
tps://github.com/FuelL$ abs/fuel-core/pull/1991): Prepare the database to use different types than `Database` for atomic view.
- [#1989](https://github.com/FuelLabs/fuel-cosh: 202: re/pull/1989): Ext-: not foundract `Historical
View` trait from the `AtomicV$ iew`.
- [#1676](https://github.com/FuelLabs/fuel-core/pull/1676): New `fuel-core-client` is incompatible with sh: 203: -: not found
$ the old `fuel-core` because of two requested new fields.
- [#1676](https://github.com/FuelLabs/fuel-core/pull/1676): Changed default value for `api-requesh: 204: -: not found
$ st-timeout` to be `30s`.
-sh: 205:  [#1676](https:/Syntax error: "(" unexpected/github.com/F
uelLabs/fuel-core/pul$ l/1676): Now, GraphQL API has complexity and depth limitash: 205: tions on the queSyntax error: "(" unexpectedries. The def
ault complexity lim$ it is `20000`. It is ~50 blocks per requestsh: 205:  with transactioSyntax error: "(" unexpectedn IDs and ~2
-5 full blocks.
$ 
### Fixed
- [#2000](https://gish: 205: Syntax error: "(" unexpectedthub.com/Fu
$ elLsh: 205: abs/fuel-core/pulSyntax error: "(" unexpectedl/2000): Use 
correct query name$  in metrics for aliased queries.

## [Vsh: 205: ersion 0.30.0]
Syntax error: "(" unexpected
### Added

- [#1975](https://$ github.com/FuelLabs/fuel-core/pull/1975): Added `DependentCost` benchmarks for the `cfe` and `cfei` opcodes.
- [#1975](https://github.com/FuelLabs/fuel-core/pull/1975): Added `DependentCost` for the `cfe` opcode to the `GasCosts` endpoint.
- [#1974](https://github.com/FuelLabs/fuel-core/pull/1974): Optimized the work of `InMemoryTransaction` for lookups and empty insertion.

### Changed
- [#1973](https://github.com/FuelLabs/fuel-core/pull/1973): Updated VM initialization benchmark to include many inputs and outputs.

#### Breaking
- [#1975](https://github.com/FuelLabs/fuel-core/pull/1975): Updated gas prices according to new release.
- [#1975](https://github.com/FuelLabs/fuel-core/pull/1975): Csh: 1: graphql-max-depth: not found
sh: 205: -: not found
$ hanged `GasCosts` endpoint to rsh: 1: graphql-max-complexity: not found
sh: 206: -: not found
$ eturn `DependentCost` for the `cfei`sh: 1: graphql-max-recursive-depth: not found
sh: 207: -: not found
$  $ $ sh: 210: opcode viSyntax error: "(" unexpecteda `cfeiDepe
ndentCost`.
- [#1975]$ (https://github.com/FuelLabs/fuel-core/pull/1975): Use `fuel-vm 0.54.0`. More information in the [release](https://github.com/FuelLabs/fuel-vm/relsh: 210: eases/tag/v0.54-: not found
$ .0).

## [Version 0.29.0]

### Added
- [#1889](https://github.com/FuelLabs/fuel-core/pull/1889): Add new `FuelGasPriceProvider` that receives the gas price algorithm from a `GasPriceService`

### Chsh: 211: -: not found
$ anged
- [#1942](https://github.com/FuelLabs/fuel-core/pull/1942): Sequential relayer's commits.
- [#1952](https://github.com/FuelLabs/fuel-core/pull/1952): Change tip sorting to ratio between tip and max gas sorting in txpool
- [#1960](https://github.com/FuelLabs/fuel-coresh: 1: lost: not found
sh: 212: -: not found
$ /pull/1960): Update fuel-vm to v0.sh: 213: 53.0.
- [#1964Syntax error: "(" unexpected](https://githu
b.com/FuelLabs/fuel-core/pull/1$ 964): Add `creation_instant` as second sort keysh: 213:  in tx pool

### Syntax error: "(" unexpectedFixed
- [#
1962](https://github.$ com/FuelLabs/fuel-core/pull/1962): Fixes the error message fosh: 213: r incorrect keypair's pSyntax error: "(" unexpectedath.
- [#1950]
(https://github.com/Fuel$ Labs/fuel-core/pull/1950): Fix curssh: 213: or `BlockHeight` encoSyntax error: "(" unexpectedding in `Sort
edTXCursor`

#$ # [Version 0.28.0]

### Changed
- [#1934](sh: 213: https://github.com/FSyntax error: "(" unexpecteduelLabs/fuel-c
ore/pull/1934): Updated $ benchmark for the `aloc` opcode to be `DependentCossh: 213: t`. Updated `vm_initialSyntax error: "(" unexpectedization` benchma
rk to exclude growing of$  memory(It is handled by VM reuse).
- [#1916sh: 213: ](https://github.comSyntax error: "(" unexpected/FuelLabs/fuel
-core/pull/1916): Spee$ d up synchronisation of the blocks f$ or the `fuel-core-sync` service.
- [#$ 1888](https://github.com/FuelLabs/fuel-core/pullsh: 215: /1888): optimization: Syntax error: "(" unexpectedReuse VM memor
y $ across executions.

#### Breaking

- [#1934](sh: 215: https://github.coSyntax error: "(" unexpectedm/FuelLabs/fue
l-core/pull/1934): C$ hanged `GasCosts` endpoint to return `Dependsh: 215: entCost` for the Syntax error: "(" unexpected`aloc` opcode 
via `alocDependentCost$ `.
- [#1934](https://github.com/FuelLabs/fuesh: 215: l-core/pull/1934):Syntax error: "(" unexpected Updated defa
ult gas costs for the loca$ l testnet configuration. All opcodes became cheapesh: 215: r.
- [#1924](https://gSyntax error: "(" unexpectedithub.com/Fuel
Labs/fuel-core/pull/19$ 24): `dry_run_opt` has new `gas_price: Optish: 215: on<u64>` arguSyntax error: "(" unexpectedment
- [#188
8](https://github.com/F$ uelLabs/fuel-core/pull/1888): Upgraded `fuel-vm` to sh: 215: `0.51.0`. See Syntax error: "(" unexpected[release](https://githu
b.com/FuelLabs/fuel-vm/r$ eleases/tag/v0.51.0) for more informa$ tion.

### Added
- [#1939]$ (https://github.com/FuelLabs/fuel-core/pull/1939): Addsh: 217: ed API functionsSyntax error: "(" unexpected to open a Roc
ksDB in different modes$ .
- [#1929](https://github.com/F$ uelLabs/fuel-core/pull/1929): Added su$ pport of customization of the $ state transition version in th$ e `ChainConfig`.

### Removed
- [#1913](https://githsh: 221: ub.com/FuelLabs/fuSyntax error: "(" unexpectedel-core/pull/1913
): Removed dead code from $ the project.

### Fixed
- [#1921](https://githush: 221: b.com/FuelLabs/fuSyntax error: "(" unexpectedel-core/pull/192
1): Fixed unstable `gossi$ psub_broadcast_tx_with_accept` test.
- [#1915]sh: 221: (https://github.Syntax error: "(" unexpectedcom/FuelLabs/
fuel-core/pull/1915)$ : Fixed reconnection issue in the$  dev cluster with AWS cluster.
- [#1$ 914](https://github.com/FuelLabs/fuel-core/pull/1914)sh: 223: : Fixed halting Syntax error: "(" unexpectedof the node du
ring synchronization $ in PoA service.

## [Version 0$ .27.0]

### Added

- [#1895](https://git$ hub.com/FuelLabs/fuel-core/pull/1895): Added sh: 225: backward and forward Syntax error: "(" unexpectedcompatibility
 integration tests for f$ orkless upgrades.
- [#1898](https://github.sh: 225: com/FuelLabs/fuel-Syntax error: "(" unexpectedcore/pull/189
8): Enforce increasing$  of the `Executor::VERSION` on each release.

sh: 225: ### Changed

Syntax error: "(" unexpected- [#1906](ht
tps://github.com/Fuel$ Labs/fuel-core/pull/1906): Makes $ `cli::snapshot::Command` members public$  such that clients can create and $ execute snapshot commands progr$ ammatically. This enables snapshot execution in external sh: 229: programs, such as theSyntax error: "(" unexpected regenesis test suite.

- [#1891](https://github.$ com/FuelLabs/fuel-core/pull/1891):$  Regenesis now preserves `FuelBl$ ockMerkleData` and `FuelBlockMerkleMetadata` in the osh: 231: ff-chain table. ThSyntax error: "(" unexpectedese tables are c
hecked when querying me$ ssage proofs.
- [#1886](https://github.com/sh: 231: FuelLabs/fuel-coreSyntax error: "(" unexpected/pull/1886): 
Use ref to `Block` i$ n validation code
- [#1876](https://github.sh: 231: com/FuelLabs/fuelSyntax error: "(" unexpected-core/pull/1876)
: Updated benchmark to in$ clude the worst scenario for `CROO` opcode. sh: 231: Also include conseSyntax error: "(" unexpectednsus parameters 
in bench output.
- [#$ 1879](https://github.com/FuelLabs/fuel-$ core/pull/1879): Return the old beh$ aviour for the `discovery_wsh: 233: orks` test.
Syntax error: "(" unexpected- [#1848](htt
ps://github.com/FuelLa$ bs/fuel-core/pull/1848): Added `version` field tsh: 233: o the `Block` andSyntax error: "(" unexpected `BlockHeader
` GraphQL entities. Add$ ed corresponding `version` field to the `$ Block` and `BlockHeader` client ty$ pes in `fuel-core-client`.
- [#1873](https://g$ it$ hub.com/FuelLabs/fuel-core/pull/1873/): Separate dry sh: 237: runs from block pSyntax error: "(" unexpectedroduction in 
executor code, remove $ `ExecutionKind` and `ExecutionType`, remove `sh: 237: thread_block_transactSyntax error: "(" unexpectedion` concept,
 remove `PartialBlockC$ omponent` type, refactor away `inner` functions.
- sh: 237: [#1900](https://gitSyntax error: "(" unexpectedhub.com/FuelLab
s/fuel-core/pull/1900): Upda$ te the root README as `fuel-core run`$  no longer has `--chain` as an op$ tion. It has been replaced by `--snapshot$ `.

#### Breaking

- [#1894](https://github.com/sh: 240: FuelLabs/fuel-core/pull/189Syntax error: "(" unexpected4): Use testn
et configuration for loc$ al testnet.
- [#1894](https://github.com/Fuesh: 240: lLabs/fuel-core/Syntax error: "(" unexpectedpull/1894): Re
moved support for helm $ chart.
- [#1910](https://github.com/FuelLash: 240: bs/fuel-core/pulSyntax error: "(" unexpectedl/1910): `fuel
-vm` upgraded to `0.5$ 0.0`. More information in the [changelog](https:/sh: 240: /github.com/FueSyntax error: "(" unexpectedlLabs/fuel-
vm/releases/tag/v0.50.0$ ).

## [Version 0.26.0]

### Fixed
$ 
#### Breaking

- [#1868](https:$ //github.com/FuelLabs/fuel-core/pull/1868): Include tsh: 242: he `event_inbox_Syntax error: "(" unexpectedroot` in the 
header hash. Changed typ$ es of the `transactions_count` to `u16` and `messsh: 242: age_receipt_count`Syntax error: "(" unexpected to `u32` inste
ad of `u64`. Updated th$ e application hash root calculation$  to not pad numbers.
- [#1866](https://git$ hub.com/FuelLabs/fuel-core/pull/1866): Fixed a sh: 244: runtime panic that ocSyntax error: "(" unexpectedcurred when r
estarting a node. The pan$ ic happens when the relayer database is$  already populated, and the relayer attem$ pts an empty commit during start up. This invash: 246: lid commit is remSyntax error: "(" unexpectedoved in this P
R.
- [#1871](https://git$ hub.com/FuelLabs/fuel-core/pull/1871): Fixedsh: 246:  `block` endpointSyntax error: "(" unexpected to return fe
tch the blocks from bot$ h databases after regenesis.
- [#1856](hsh: 246: ttps://github.com/Syntax error: "(" unexpectedFuelLabs/fuel-co
re/pull/1856): Replac$ ed instances of `Union` with `En$ um` for GraphQL definitions of `ConsensusParamet$ ersVersion` and related types. This is$  needed because `Union` does not sup$ port multiple `Version`s inside$  discriminants or empty variants.
- [sh: 251: #1870](https://gitSyntax error: "(" unexpectedhub.com/FuelL
abs/fuel-core/pull/$ 1870): Fixed benchmarks for the `0.25.3`.
sh: 251: - [#1870](https:Syntax error: "(" unexpected//github.com/
FuelLabs/fuel-core/p$ ull/1870): Improves the performanc$ e of getting the size of the co$ ntract from the `InMemoryTransaction`.
- [#1$ 851](https://github.com/FuelLabs/fuel-core/pull/1851/): Prsh: 254: ovided migration capSyntax error: "(" unexpectedabilities (en
abled addition of new colu$ mn families) to RocksDB instance.

### Added

- sh: 254: [#1853](https://githSyntax error: "(" unexpectedub.com/Fuel
Labs/fuel-core/pull/185$ 3): Added a test case to verify the databash: 254: se's behavior wheSyntax error: "(" unexpectedn new column
s are added to the Ro$ cksDB database.
- [#1860](https://github.com/Fsh: 254: uelLabs/fuel-core/pull/18Syntax error: "(" unexpected60): Regenesis
 now preserves `FuelBlockIdsT$ oHeights` off-chain table.

### Changed

sh: 254: - [#1847](https://gSyntax error: "(" unexpectedithub.com/Fue
lLabs/fuel-core/pull/1$ 847): Simplify the validation interface to use `sh: 254: Block`. Remove `ValiSyntax error: "(" unexpecteddation` varian
t of `ExecutionKind`.
- [#1$ 832](https://github.com/FuelLabs/fuel-core/pull/1832): sh: 254: Snapshot geneSyntax error: "(" unexpectedration can be
 cancelled. Progress $ is also reported.
- [#1837](https://github.com/FuelLabsh: 254: s/fuel-core/pull/1Syntax error: "(" unexpected837): Refactor 
the executor and separa$ te validation from the other use cas$ es

## [Version 0.25.2]

### Fi$ xed

- [#1844](https://github.com$ /FuelLabs/fuel-core/pull/1844): Fixed the publissh: 257: hing of the `fueSyntax error: "(" unexpectedl-core 0.25.1`
 release.
- [#1842]$ (https://github.com/FuelLabs/fuel-core/pull/1842)sh: 257: : Ignore RUSTSECSyntax error: "(" unexpected-2024-0336: `ru
stls::ConnectionCommon$ ::complete_io` could fall into an infinite loosh: 257: p based on networkSyntax error: "(" unexpected

## [Versi
on 0.25.1]

###$  Fixed

- [#1840](https://github.com$ /FuelLabs/fuel-core/pull/1840): Fixe$ d the publishing of the `fuel-core 0.25.0`$  release.

## [Version 0.25.0]

$ ### Fixed

- [#1821](https://github.com/Fu$ elLabs/fuel-core/pull/1821): C$ an handle missing tables in snapshot.$ 
- [#1814](https://github.com/FuelLabs/fuel-core/pull/1814): Bush: 264: gfix: the `iter_aSyntax error: "(" unexpectedll_by_prefix` 
was not working for al$ l tables. The change adds a `Rust` level filtering.
sh: 264: 
### Added

- [#1Syntax error: "(" unexpected831](https://
github.com/FuelLabs/fue$ l-core/pull/1831): Included the total gas and fee used by tsh: 264: ransaction into `Syntax error: "(" unexpectedTransactionS
tatus`.
- [#18$ 21](https://github.com/FuelLabs/fuel-coresh: 264: /pull/1821): PropagatSyntax error: "(" unexpectede shutdow
n signal to (re)$ genesis. Also add progress bar forsh: 264:  (re)genesis.
- [#1Syntax error: "(" unexpected813](https:/
/github.com/FuelLabs/fuel-c$ ore/pull/1813): Added back support for `/health` sh: 264: endpoint.
- [#1Syntax error: "(" unexpected799](https://gith
ub.com/FuelLabs/fuel-$ core/pull/1799): Snapshot creation is now concurrent.
-sh: 264:  [#1811](https://github.Syntax error: "(" unexpectedcom/FuelLabs/fuel-
core/pull/1811): Regen$ esis now preserves old blocks and transactions f$ or GraphQL API.

### Changed

- $ [#1833](https://github.com/FuelLabs/fue$ l-core/pull/1833): Regenesis of `SpentMessages` and `Psh: 267: rocessedTransSyntax error: "(" unexpectedactions`.
- 
[#1830](https://github.$ com/FuelLabs/fuel-core/pull/1830): Use versioningsh: 267:  enum for WASM exeSyntax error: "(" unexpectedcutor inpu
t and output.
$ - [#1816](https://github.co$ m/FuelLabs/fuel-core/pull$ /1816): Updated the upg$ radable executor to fetch the state sh: 270: transition bytecode fSyntax error: "(" unexpectedrom the dat
abase when the$  version doesn't match a nativsh: 270: e one. This chSyntax error: "(" unexpectedange enable
s the WASM exe$ cutor in the "production" build andsh: 270:  requires aSyntax error: "(" unexpected `wasm32-un
known-unknown$ ` target.
- [#1812](h$ ttps://github.com/FuelLabs$ /fuel-core/pull/1812): $ Follow-up PR to simplify $ the logic around paralle$ l snapshot creation.
- [#1809](hsh: 275: ttps://githSyntax error: "(" unexpectedub.com/F
uelLabs/fuel-$ core/pull/1809): Fetch `Consensush: 275: sParameters` from the daSyntax error: "(" unexpectedtabase

- [#1808](https://github.co$ m/FuelLabs/fuel-core/pu$ ll/1808): Fetch consensus p$ arameters from the provid$ er.

#### Breaking

$ - [#1826](https://github$ .com/FuelLabs/fuel-core/pull/1sh: 280: 826): The Syntax error: "(" unexpectedchanges ma
ke the state t$ ransition bytecode part o$ f the `ChainConfig`. It gu$ arantees the state transi$ tion's availability for the network'$ s first blocks.
    Th$ e change has many minor improvesh: 285: ments in diffeSyntax error: "(" unexpectedrent are
as related to $ the state transition bytecode:
 sh: 285:    - The stateSyntax error: "(" unexpected transition
 bytecode li$ es in its own file(`sta$ te_transition_bytecode.w$ asm`) along with the ch$ ain config file. The `ChainConfig` sh: 288: loads it autoSyntax error: "(" unexpectedmatically
 when `ChainC$ onfig::load` is called and pushessh: 288:  it back whSyntax error: "(" unexpecteden `ChainCo
nfig::write` i$ s called.
    - The `fuel-csh: 288: ore` releaseSyntax error: "(" unexpected bundle al
so contains$  the `fuel-core-wasm-executorsh: 288: .wasm` file Syntax error: "(" unexpectedof the cor
responding exec$ utor version.
    - The regenesh: 288: sis process noSyntax error: "(" unexpectedw consider
s the $ last block produced by t$ he previous network. Wh$ en we create a (re)genesi$ s block of a new network, it has sh: 291: the `height =Syntax error: "(" unexpected last_bloc
k_of_old_net$ work + 1`. It continues the oldsh: 291:  network and Syntax error: "(" unexpecteddoesn't o
verlap block$ s(before, we had `old_block.heightsh: 291:  == new_gSyntax error: "(" unexpectedenesis_b
lock.height`$ ).
    - Along with the new blosh: 291: ck height, thSyntax error: "(" unexpectede regenesis
 process also$  increases the state transitiosh: 291: n bytecode aSyntax error: "(" unexpectednd consen
sus parameters$  versions. It guarantees that sh: 291: a new networSyntax error: "(" unexpectedk doesn'
t use values$  from the previous netw$ ork and allows us not to$  migrate `StateTransition$ BytecodeVersions` and `ConsensusPsh: 294: arametersVerSyntax error: "(" unexpectedsions` tab
les.
    - Added a$  new CLI argument, `native-executor-version,` that allows overriding of the default version of the native executor. It can be useful for side rollups that have their own history of executor upgrades.
    - Replaced:

      ```rust
               let sh: 294: file = std::The: not foundfs::File::
open(path)?;
       $         let mut snapshot: Self = serde_json::sh: 295: from_readerSyntax error: "(" unexpected(&file)?;
     
 ```

   $    with a:

      ```rust
               let mut json = String::new();
               std::fs::File::open(&path)
                   .with_context(|| format!("Could not open snapshot file: {path:?}"))?
                   .read_to_string(&mut json)?;
               let mut snapshot: Self = serde_json::from_str(json.as_str())?;
      ```
      because it is 100 times faster for big JSON files.
    - Updated all tests to use `Config::local_node_*` instead of working with the `SnapshotReader` directly. It is the preparation of the tests for the futures bumps of the `Executor::VERSION`. When we increase the version, all tests continue to use `GenesisBlocksh: 1: .state_transition_bytecfuel-core: not foundode = 0` while
 the version is different, which forces the usage of the WASM executor, while for tests, we still prefer to test native execution. The `Config::local_node_*` handles it and forces the executor to use the native version.
    - Reworked the `build.rs` file of the upgradable executor. The script now caches WASM bytecode to avoid recompilation. Also, fixed the issue with outdated WASM bytecode. The script reacts on any modifications of the `fuel-core-wasm-executor` and forces recompilation (it is why we need the cache), so WASM bytecode always is actual now.
- [#1822](https://github.com/FuelLabs/fuel-core/pull/1822): Removed support of `Create` transaction from debugger since it doesn't have any script to execute.
- [#1822](https://github.com/FuelLabs/fuel-core/pull/1822): Use `fuel-vm 0.49.0` with new transactions types - `Upgrade` and `Upload`. Also added `max_bytecode_subsections` field to the `ConsensusParameters` to limit the number of bytecode subsections in the state transition bytecode.
- [#1816](https://github.com/FuelLabs/fuel-core/pull/1816): Updated the upgradable executor to fetch the state transition bytecode from the database when the version doesn't match a native one. This change enables the WASM executor in the "production" build and reqush: 1: ires a `wasm32-unknown-fuel-core-wasm-executor.wasm: not foundunknown` targe
t.

## [Version 0.24.2]

### Changed

#### Breaking
- [#1798](https://github.com/FuelLabs/fuel-core/pull/1798): Add nonce to relayed transactions and also hash full messages in the inbox root.

### Fixed

- [#1802](https://github.com/FuelLabs/fuel-core/pull/1802): Fixed a runtime panic that occurred when restarting a node. The panic was caused by an invalid database commit while loading an existing off-chain database. The invalid commit is removed in this PR.
- [#1803](https://github.com/FuelLsh: 295: abs/fuel-core/pull/1-: not found803): Produce
 block when da height haven't changed.
- [#179$ 5](https://github.com/FuelLabs/fuel-core/pull/1795): Fixed the building of the `fuel-core-wasm-sh: 296: executor` to workSyntax error: "(" unexpected outside of t
he `fuel-core` context.$  The change uses the path to the manifest file of the `fuel-core-upgradable-executor` to build the `fuel-c> ore-wasm-executor` instead of relying on the workspace.

## > [Version 0.24.1]

### Added

> - [#1787](https://github.com/Fu> elLabs/fuel-core/pull/1787): Handle processing of re> layed (forced) transactions
- [#1786> ](https://github.com/FuelLa> bs/fuel-core/pull/1786): Regene> sis now includes off-chain tabl> es.
- [#1716](https://github.co> m/FuelLabs/fuel-core/pull/1716> ): Added support of WASM state tra> nsition along with upgradable executi> on that works with native(std) and W> ASM(non-std) executors. The `fuel-core` now > requires a `wasm32-unknown-unknown` target to build.
- [#1770](https://github.com/FuelLabs/fuel-core/> pull/1770): Add the new L1 event type for for> ced transactions.
- [#1767](ht> tps://github.com/FuelLabs/fuel-core/p> ull/1767): Added consensus parameters version and state transition version to the `ApplicationHeader` to d> escribe what was used to produce this block.
- [#1760](https://github.com/FuelL> abs/fuel-core/pull/1760): Added tests to verify that the network operates with a custom chain id and base asset id.
- [#1752](https://github.com/FuelLabs/fuel-core/pull/1752): Add `ProducerGasPrice` trait that the `Producer` depends on to get the gas prish: 296: ce for the block.-: not found
- [#1747](ht
tps://github.com/FuelLabs/fue$ l-core/pull/1747): The DA bsh: 318: lock height is Syntax error: "(" unexpectednow included 
in the genesis state.
$ - [#1740](https://github.com/FuelLabs/fuel-core/pull/174sh: 318: 0): Remove optioSyntax error: "(" unexpectednal fiel
ds from genesis con$ figs
- [#1737](https://github.com$ /FuelLabs/fuel-core/pull/1737): Re$ move temporary tables for calculat$ ing roots during genesis.
- [#17$ 31](https://github.com/FuelLabs/f$ uel-core/pull/1731): Expose `schem$ a.sdl` from `fuel-core-client`.

### Changedsh: 324: 

#### BreakingSyntax error: "(" unexpected

- [1785](h
ttps://github.com/F$ uelLabs/fuel-core/pull/1785): Prod$ ucer will only include DA height if$  it has enough gas to include the $ associate forced transactions.
- [#1771](https://gish: 327: thub.com/FuelLabSyntax error: "(" unexpecteds/fuel-core/p
ull/1771): Contract$  'states' and 'balances' brought back into sh: 327: `ContractConfig`Syntax error: "(" unexpected. Parquet now 
writes a file per t$ able.
- [1779](https://github.com/FuelLabs/fuel-core/sh: 327: pull/1779): ModiSyntax error: "(" unexpectedfy Relayer se
rvice to order Even$ ts from L1 by block index
- [#178$ 3](https://github.com/FuelLabs/fuel-$ core/pull/1783): The PR upgrade $ `fuel-vm` to `0.48.0` release. Beca$ use of some breaking changes, we a$ lso adapted our codebase to follow them:
sh: 332:   - ImplementatSyntax error: "(" unexpectedion of `Defau
lt` for configs was$  moved under the `test-helpers` feature. Tsh: 332: he `fuel-core` bSyntax error: "(" unexpectedinary uses te
stnet configuration$  instead of `Default::default`(for cases when `ChainCsh: 332: onfig` was not pSyntax error: "(" unexpectedrovided by th
e user).
  - All$  parameter types are enums now and require sh: 332: corresponding modSyntax error: "(" unexpectedifications ac
ross the codebase(w$ e need to use getters and setters). The GraphQL Ash: 332: PI remains the sSyntax error: "(" unexpectedame for simp
licity, but each p$ arameter now has one more field - `version`, tsh: 332: hat can be used Syntax error: "(" unexpectedto decide how
 to deserialize.
$   - The `UtxoId` type now is 34 bytes instead osh: 332: f 33. It affectsSyntax error: "(" unexpected hex represen
tation and require$ s adding `00`.
  - The `block_gas_limit`sh: 332:  was moved to `CSyntax error: "(" unexpectedonsensusPara
meters` from `Chai$ nConfigsh: 332: `. It means the Syntax error: "(" unexpectedblock produc
er doesn't specify $ the block gas limit anymore, and we don't nsh: 332: eed to propagatSyntax error: "(" unexpectede this infor
mation.
  - The `$ bytecodeLength` field is removed from the sh: 332: `Create` transaSyntax error: "(" unexpectedction.
  - 
Removed `ConsensusP$ arameters` from executor config bec$ ause `ConsensusPar$ ameters::default` is not available$  anymore. Instead, executors fetch$  `ConsensusParameters` from the $ database.

- [#1769](https://github.com/Fuesh: 337: lLabs/fuel-coreSyntax error: "(" unexpected/pull/1769)
: Include new fiel$ d on header for the merkle root of imported evesh: 337: nts. Rename othSyntax error: "(" unexpecteder message r
oot field.
- [#17$ 68](https://github.com/FuelLabs/fuel-core/push: 337: ll/1768): MovedSyntax error: "(" unexpected `ContractsI
nfo` table to the o$ ff-chain database. Removed `salt` field from thsh: 337: e `ContractConfiSyntax error: "(" unexpectedg`.
- [#176
1](https://github.co$ m/FuelLabs/fuel-core/pull/1761): Adjustments to the upcoming testnet configs:
  - Decreased the max size of thsh: 337: e contract/predSyntax error: "(" unexpectedicate/script 
to be 100KB.
  - De$ creased the max size of the transaction to be 110KB.
  - Decreased thesh: 337:  max number of sSyntax error: "(" unexpectedtorage slots t
o be 1760(110KB / 6$ 4).
  - Removed fake coins from the genesis state.
  - Renamed folders to be "testnet" and "dev-testnet".
  - The name of the networks are "Upgradable Testnet" and "Upgradable Dev Testnet".

- [#1694](https://github.com/FuelLabs/fuel-core/pull/1694): The change moves the database transaction logic from the `fuel-core` to the `fuel-core-storage` level. The corresponding [issue](https://github.com/FuelLabs/fuel-core/issues/1589) described the reason behind it.

    ## Technical details of implementation

    - The change splits the `KeyValueStore` into `KeyValueInspect` and `KeyValueMutate`, as well the `Blueprint` into `BlueprintInspect` and `BlueprintMutate`. It allows requiring less restricted constraints for any read-related operations.

    sh: 1: - One of the main ideas UtxoId: not foundof the change
 is to allow for the actual storage only to implement `KeyValueInspect` and `Modifiable` without the `KeyValueMutate`. It simplifies work with the databases and provides a safe way of interacting with them (Modification into the database can only go through the `Modifiable::commit_changes`). This feature is used to [track the height](https://github.com/FuelLabs/fuel-core/pull/1694/files#diff-c95a3d57a39feac7c8c2f3b193a24eec39e794413adc741df36450f9a4539898) of each database during commits and even limit how commits are done, providing additional safety. This part of the change was done as a [separate commit](https://github.com/FuelLabs/fuel-core/pull/1694/commits/7b1141ac838568e3590f09dd420cb24a6946bd32).

    - The `StorageTransaction` is a `StructuredStorage` that uses `InMemoryTransaction` inside to accumulate modifications. Only `InMemoryTransaction` has a real implementation of the `KeyValueMutate`(Other types only implement it in tests).

    - The implementation of the `Modifiable` for the `Database` contains a business logic that provides additional safety but limits the usage of the database. The `Database` now tracks its height and is sh: 1: responsible for its updates00: not found. In the `comm
it_changes` function, it analyzes the changes that were done and tries to find a new height(For example, in the case of the `OnChain` database, we are looking for a new `Block` in the `FuelBlocks` table).

    - As was planned in the issue, now the executor has full control over how commits to the storage are done.

    - All mutation methods now require `&mut self` - exclusive ownership over the object to be able to write into it. It almost negates the chance of concurrent modification of the storage, but it is still possible since the `Database` implemensh: 337: ts the `Clone` trait. -: not foundTo be sure tha
t we don't corrupt the state of the database, the `co$ mmit_changes` function implements additional safety checks to be sure that we commit updates per each height only once time.

    - Side changes:
      - The `drop` function was moved from `Database` to `RocksDB` as a preparation for the state rewind since the read view should also keep the drop function until it is destroyed.
      - The `StatisticTable` table lives in the off-chain worker.
      - Removed duplication of the `Database` from the `dap::ConcreteStorage` since it is already available from the VM.
      - The executor return only produced `Chansh: 1: block_gas_limit: not found
sh: 1: ConsensusParameters: not found
sh: 1: ChainConfig: not found
sh: 338: -: not found
$ ges` instead of the storage transaction, which simplifies the interactionsh: 1: bytecodeLength: not found
sh: 1: Create: not found
sh: 339: -: not found
$  between modules and port definition.
      - The logic related to the iteration over the storage is moved to the `fuel-core-storage` crate and is now reusable. It provides an `iterator`sh: 1: ConsensusParameters: not found
sh: 1: ConsensusParameters::default: not found
sh: 1: ConsensusParameters: not found
sh: 340: -: not found
$  $ method that duplicates the logic sh: 342: from `MemoryStore` onSyntax error: "(" unexpected iterating ov
er the `BTreeMap` and methods like$  `iter_all`, `iter_all_by_prefix`, etc. It was done in a sepsh: 342: arate revivable [commit]Syntax error: "(" unexpected(https://git
hub.com/FuelLabs/fuel-c$ ore/pull/1694/commits/5b9bd78320e6f36d0650ec05698fsh: 342: 12f7d1b3c7c9).
Syntax error: "(" unexpected      - The `Mem
oryTransactionView` is $ fully replaced by the `StorageTransactionInner`.
      - Removed `flush` method from the `Database` sincesh: 342:  it is not needed -: not foundafter https://gi
thub.com/FuelLabs/fuel-core$ /pull/1664.

- [#1693](https://github.com/FuelLabs/fuel-core/pull/1693)sh: 343: -: not found
$ sh: 344: : The change seSyntax error: "(" unexpectedparates the i
nitial chain state fro$ m the chain config and stores them in separate files when generatinsh: 344: -: not found
$ g a snapshot. The state snapshot can be generated in a sh: 345: -: not found
$ new format where parquet is used for compression and indexing while postcard is usedsh: 346: -: not found
$  $ for encoding. This enablsh: 348: es importing in aSyntax error: "(" unexpected stream like f
ashion which reduces memo$ ry requirements. Json encoding is sti$ ll supported to enable easy manual setup.$  However, parquet is preferred f$ or large state files.

  ### Snapshot command

  The CLI was expanded to allow customizing the used encoding. Snapshots are now generated along with a metadata file describing the encoding used. The metadata file contains encoding details as well as the location of additional files inside the snapshot directory containing the actual data. The chain config is always sh: 1: KeyValueStore: not found
sh: 1: KeyValueInspect: not found
sh: 1: KeyValueMutate: not found
sh: 1: Blueprint: not found
sh: 1: BlueprintInspect: not found
sh: 1: BlueprintMutate: not found
sh: 351: -: not found
$ $ generated in the JSON format.

  The snapshot command nowsh: 353:  has the '--output-Syntax error: "(" unexpecteddirectory' for s
pecifying where to save th$ e snapshot.

  ### Run command

$   The run command now includes the 'db_prune' flag which when provided will prune the existing db and start genesis from tsh: 354: he provided snapSyntax error: "(" unexpectedshot metadata 
file or the local testne$ t configuration.

  The snapshot met$ adata file contains paths to the chain config file and files containing chain state items (coins, messages, contracts, contract states, and balances), whish: 355: ch are loaded viaSyntax error: "(" unexpected streaming.


  Each item group in the gen$ esis process is handled by a separa$ te worker, allowing for parallel loading. Workers stream file contents in batches.

  A database transaction is committed every time an item group ish: 356: s successfully load-: not founded. Resumabili
ty is achieved by recording the l$ ast loaded group index within the sam$ e db tx. If loading is aborted, the remaining workers are shutdown. Upon restash: 1: rt, workers resumeSyntax error: "&" unexpected from the la
st processed group.

  ### Co$ ntract States and Balances

  Using unifo$ rm-sized batches may result in batches containing items from multiple contracts. Optimal performance can presumably be achieved by selecting a batch size that typically encompasses an entire contract's state or balance, allowing for immediate initialization sh: 359: of relevant Merkle-: not found trees.

#
## Removed

- [#1757](http$ s://github.com/FuelLabs/fuel-core/pull/1757): Removed `protobuf` from everywhere since `libp2p` uses `quick-protobuf`.

## [Version 0.23.0]

### Added

- [#1713](https://github.com/FuelLabs/fuel-core/pull/1713): Added automatic `impl` of traits `StorageWrite` and `StorageRead` for `StructuredStorage`. Tables that use a `Blueprint` can be read and written using these interfaces provided by structured storage types.
- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): Added a new `Merklized` blueprint that maintains the binary Merkle tree over the storage data. It supports only the insertion of the objects without removing them.
- [#1657](https://github.com/FuelLabs/fuel-core/pull/1657): Moved `ContractsInfo` table from `fuel-vm` to on-chain tables, and created version-able `ContractsInfoType` to act as the table's data type.

### Changed

- [#1872](https://sh: 1: drop: not found
sh: 1: Database: not found
sh: 1: RocksDB: not found
sh: 360: -: not found
$ github.com/FuelLabs/fuel-core/pull/1872): Added Eq and PartialEq dsh: 1: StatisticTable: not found
sh: 361: -: not found
$ erives to TransactionStatus and TransactionResponse to enable comparison in the e2e tests.
- [#1723](https://github.com/Fsh: 1: Database: not found
sh: 1: dap::ConcreteStorage: not found
sh: 362: -: not found
$ uelLabs/fuel-core/pull/1723): Notify about imported blocks from the off-chain worker.
- [#1717](https://github.com/FuelLabs/fuel-core/pull/1717): The fix for sh: 1: Changes: not found
sh: 363: -: not found
$ the [#1657](https://github.com/FuelLabs/fuel-core/pull/1657) to include the contract into `ContractsInfo` tablsh: 364: e.
- [#1657](https://gSyntax error: "(" unexpectedithub.com/FuelL
abs/fuel-core/pull/1657): Upg$ rade to `fuel-vm` 0.46.0.
- [#1671](https://github.com/FuelLabs/fuel-core/pull/1671): The logic related to the `FuelBlockIdsToHeights` is moved to the off-chain worker.
- [#1663](https://github.com/FuelLabs/fuel-core/pull/1663): Reduce the punishment criteria for mempool gossipping.
- [#1658](https://github.com/FuelLabs/fuel-core/pull/1658): Resh: 1: MemoryTransactionView: not found
sh: 1: StorageTransactionInner: not found
sh: 364: -: not found
$ moved `Receipts` table. Instead, receipts are part of the `TransactionStatuses` table.
- [#1640](https://github.com/FuelLabs/fuelsh: 1: flush: not found
sh: 1: Database: not found
sh: 365: -: not found
$ $ -core/pull/1640): Upgrade to fuel-vm 0.45.0.
- sh: 367: [#1635](https://Syntax error: "(" unexpectedgithub.com/Fue
lLabs/fuel-core/pull/$ 1635): Move updating of the owned m$ essages and coins to off-chain worker.$ 
- [#1650](https://github.com/FuelL$ abs/fuel-core/pull/1650): Add api endpoint for getting estimates for future gas prices
- [#1649](https://github.com/FuelLabs/fuel-core/pull/1649): Add api endpoint for getting latest gas price
- [#1600](https://github.com/FuelLabs/fuel-core/pull/1640): Upgrade to fuel-vm 0.45.0
- [#1633](https://githsh: 370: ub.com/FuelLabs/fueThe: not foundl-core/pull/
1633): Notify services about $ impo$ rting of the genesis block.
- [#1625](https://github.com/FuelLabs/fuel-core/pull/1625): Making relayer independent from the executor and preparation for the force transaction inclusion.
- [#1613](https://github.com/FuelLabs/fuel-core/pull/1613): Add apsh: 372: i endpoint to retrThe: not foundieve a messag
e by its nonce.
- [#16$ 12](https://github.com/FuelLa$ bs/fuel-core/pull/1612): Use `AtomicVi$ ew` in all services for consisten$ t results.
- [#1597](https://github.com/FuelLabs/fuel-core/pull/1597): Unify namespacing for `libp2p` modules
- [#1591](https://github.com/FuelLabs/fuel-core/pull/1591): Simplify libp2p dependencies and not depend on all sub modules directlysh: 376: The: not found
$ .$ 
- [#1590](https://github.com/Fush: 378: elLabs/fuel-core/puSyntax error: "(" unexpectedll/1590): Use `
AtomicView` in the `TxPoo$ l` to read the state of the database d$ uring insertion of the transactions.
- [#1587](https://github.com/FuelLabs/fuel-core/pull/1587): Use `BlockHeight` as a primary key for the `FuelsBlock` table.
- [#1585](https://github.com/FuelLabs/fuel-core/pull/1585):sh: 379: Each: not found
$  $ Let `NetworkBehaviour` macro generate `FuelBehaviorEvent` in p2p
- [#1579](https://github.com/FuelLabs/fuel-core/pull/1579): The change extracts the off-chain-related logic from the executor and moves it to the GraphQL off-chain worker. It creates two new concepts - Off-chain and On-chain databsh: 381: A: not found
$ $ a$ ses where the GraphQL worker has e$ xclusive ownership of the database and may modi> fy it without intersecting with th> e On-chain database.
- [#1577](h> ttps://github.com/FuelLabs/fuel-> core/pull/1577): Moved insertion of sealed blocks into the>  `BlockImporter` instead of > the executor.
- [#1574](https://github.com/Fuel> Labs/fuel-core/pull/1> 574): Penalizes peers for sending inva> lid responses or for not reply> ing at all.
- [#1601](https://github.com/FuelLabs/fuel-core/pull/1601): Fix for> matting in docs and ch> eck that `cargo doc` passes in the CI.
- [#1636](https://github.com/FuelLabs/fuel-core/pull/1636): Add more docs to GraphQL DAP API.

#### Breaking

- [#1725](https://github.com/FuelLabs/fuel-core/pull/1725): All API endpoints now are prefixed with `/v1` version. New usage looks like: `/v1/playground`, `/v1/graphql`, `/v1/graphql-sub`, `/v1/metrics`, `/v1/health`.
- [#1722](https://github.com/FuelLabs/fuel-core/pull/1722): Bugfixsh: 385: : Zero `predicate_gasUsing: not found_used` field d
uring validation of the produced b$ lock.
- [#1714](https://github.com/FuelLa$ bs/fuel-core/pull/1714): The ch$ ange bumps the `fuel-vm` to `0.47.1$ `. It breaks several breaking changes into the protocol:
 sh: 401:  - All malleable fieSyntax error: "(" unexpectedlds are z
ero du$ ring the execution and unavailable through the GTFsh: 401:  getters. AccessinSyntax error: "(" unexpectedg them via the 
memory directly is sti$ ll possible, but they are zero.
  - The `Transactiosh: 401: n` doesn't define tSyntax error: "(" unexpectedhe gas price a
nymore. The gas price$  is defined by the block producer and recorded in tsh: 401: he `Mint` transactSyntax error: "(" unexpectedion at the en
d of the block. A price$  of future blocks can be fetched through ash: 401:  [new API nedopointSyntax error: "(" unexpected](https://gith
ub.com/FuelLabs/fuel-$ core/issues/1641) and the price of the last blocksh: 401:  can be fetch or viaSyntax error: "(" unexpected the block
 or another [API endpoi$ nt](https://github.com/FuelLabs/fuel-core/issues/1647sh: 401: ).
  - The `GasPrSyntax error: "(" unexpectedice` policy i
s replaced wit$ h the `Tip` policy. The user mash: 401: y specify in thSyntax error: "(" unexpectede native to
kens how much he wants t$ o pay the block producer to include hish: 401: s transaction in thSyntax error: "(" unexpectede block. It i
s the prioritization $ mechanism to incentivize the block producer to include usersh: 401: s transactions eaSyntax error: "(" unexpectedrlier.
  -
 The `MaxFee` policy is man$ datory to set. Without it, the transaction pool wilsh: 401: l reject the tranSyntax error: "(" unexpectedsaction. Since 
the block producer def$ ines the gas price, the only way to control how mush: 401: ch user agreed to Syntax error: "(" unexpectedpay can be done
 only$  through this policy.
  - The `maturity` field sh: 401: is removed from thSyntax error: "(" unexpectede `Input::Coin`
. The same affect can be$  achieve with the `Maturity` policy on the transactiosh: 401: n and predicateSyntax error: "(" unexpected. This change
s breaks how input coi$ n is created and removes the passing of this argument.
sh: 401:   - The metadataSyntax error: "(" unexpected of the `Checked<T
x>` doesn't contain `ma$ x_fee` and `min_fee` anymore. Only `max_gas` andsh: 401:  `min_gas`. The `maxSyntax error: "(" unexpected_fee` is contr
olled by the user via$  thsh: 401: e `MaxFee` policy.
Syntax error: "(" unexpected  - Added autom
atic `impl` of trait$ s `StorageWrite` and `StorageRead` for `Structuresh: 401: dStorage`. Tables Syntax error: "(" unexpectedthat use a `Bluep
rint` can be read and wri$ tten using these interfaces provided by structured ssh: 401: torage types.

Syntax error: "(" unexpected- [#1712](htt
ps://github.com$ /FuelLabs/fuel-core/pull/1712): sh: 401: Make `ContractSyntax error: "(" unexpectedUtxoInfo
` type a version-able en$ um for use in the `ContractsLatestUtxo`table.
-sh: 401:  [#1657](https://gSyntax error: "(" unexpectedithub.com/FuelLabs/fu
el-core/pull/1657): Ch$ anged `CROO` gas price type from `Word` to `DependentGasPrice`. Tsh: 401: he dependent gas prSyntax error: "(" unexpectedice values are
 dummy values while$  awaiting updated benchmarks.
- [#1671](sh: 401: https://github.com/Syntax error: "(" unexpectedFuelLabs/fuel-c
ore/pull/1671): The G$ raphQL API uses block height instead of the block id wsh: 401: here it is possible. Syntax error: "(" unexpectedThe transactio
n status contains `b$ lock_height` instead of the `block_id`.
- [#16sh: 401: 75](https://gSyntax error: "(" unexpectedithub.com/Fue
lLabs/fuel-core/pull/167$ 5): Simplify GQL schema by disabling contract rsh: 401: esolvers in most cSyntax error: "(" unexpectedases, and jus
t return a ContractId sca$ lar instead.
- [#1658](https://github.c$ om/FuelLabs/fuel-core/pull/1658): Receipt$ s are part of the transaction status.
$     Removed `reason` from the `TransactionExsh: 404: ecutionResult::FaileSyntax error: "(" unexpectedd`. It can be
 calculated based on t$ he program state and receipts.
    Also, it is not possible to fetch `receipts` from the `Transaction` directly anymore. Instead, you need to fetsh: 404: ch `status` and itSyntax error: "(" unexpecteds receipts.

- [#1646](https://github.c$ om/FuelLabs/fuel-core/pull/1646): Remove redundant receiptssh: 404:  from queries.
- [#1Syntax error: "(" unexpected639](https://gi
thub.com/FuelLabs/fue$ l-core/pull/1639): Make Merkle metadata, i.e. `SparseMerkleMetadata` and `DenseMerkleMetadata` type version-able enums
- [#1632](https://github.com/FuelLabs/fuesh: 404: l-core/pull/1632): M-: not foundake `Message`
 type a version-able enum
- [#1631]($ https://github.com/FuelLabs/fuel-core/pull/1631): Modify api endpoint to dry run multiple transactions.
- [#16> 29](https://github.com/FuelLabs/fuel-core/pull/1629): U> se a separate database for each data domain> . Each database has its own folder where data is stored.
- [#1628](https://github.com/Fuel> Labs/fuel-core/pull/1628): Make `CompressedCoin` type a version-able enum
- [#1616](https://github.com/FuelLabs/fuel-core/pull/1616): Make `BlockHeader` type a version-able enum
- [#1614](https://github.com/FuelLabs/fuel-core/pull/1614): Use the default consensus key regardless of trigger mode. The change is breaking because it removes the `--dev-keys` argument. If the `debug` flag is set, the default consensus key will be used, regardless of the trigger mode.
- [#1596](https://github.com/FuelLabs/fuel-core/pull/1596): Make `Consensus` type a version-able enum
- [#1593](https://github.com/FuelLabs/fuel-core/pull/1593): Make `Block` type a version-able enum
- [#1576](https://github.com/FuelLabs/fuel-core/pull/1576): The change moves the implementation of thsh: 1: e storage traits for requTransaction: not foundired tables fr
om `fuel-core` to `fuel-core-storage` crate. The change also adds a more flexible configuration of the encoding/decoding per the table and allows the implementation of specific behaviors for the table in a much easier way. It unifies the encoding between database, SMTs, and iteration, preventing mismatching bytes representation on the Rust type system level. Plus, it increases the re-usage of the code by applying the same blueprint to other tables.

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

    It is a definition of the blueprint for the `ContractsRawCode` table. It has a plain blueprint meaning it simply encodes/decodes bytes andsh: 1:  max_fee: not found
sh: 1: min_fee: not found
sh: 1: max_gas: not found
sh: 1: min_gas: not found
sh: 1: max_fee: not found
sh: 1: MaxFee: not found
sh: 405: -: not found
$ stores/loads them into/from the storage. As a key codec and value codec, it uses a `Raw` encoding/decoding that simplifies writing bytes and loads them back into the memory without applying any serialization orsh: 1: impl: not found
sh: 1: StorageWrite: not found
sh: 1: StorageRead: not found
sh: 1: StructuredStorage: not found
sh: 1: Blueprint: not found
sh: 410: -: not found
$  $ sh: 412: Syntax error: "(" unexpecteddeserial
ization algorithm.

$     If the table implements `TableWithBlueprint` ansh: 412: d the selected cSyntax error: "(" unexpectedodec satisfie
s all blueprint req$ uirements, the correspondsh: 412: ing storage traiSyntax error: "(" unexpectedts for that tab
le are implemented o$ n the `StructuredStorage` type.

    ### Csh: 412: odecs

    ESyntax error: "(" unexpectedach blueprint
 allows customizing$  the key and value codecs. It allows the ush: 412: se of different cSyntax error: "(" unexpectedodecs for di
fferent tables, t$ aking into account the complexity and weight of the data and providing a way of more optimal implementation.

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

    /// Define the table for V2 value encoding/decosh: 1: reason: not found
sh: 1: TransactionExecutionResult::Failed: not found
sh: 412: Removed: not found
$ ding.
    /// It uses `Postcard` codec for the value instead of `Raw` codec.
    ///
    /// # Dev-note: The columns is the same.
    impl Tablesh: 1: receipts: not found
sh: 1: Transaction: not found
sh: 1: status: not found
sh: 413: Also,: not found
$ WithBluesh: 414: print for ContractsRawSyntax error: "(" unexpectedCodeV2 {
 
       type Blueprint = Plai$ n<Raw, Postcard>;

        fn cosh: 414: lumn() -> Column Syntax error: "(" unexpected{
            
Column::ContractsRawCod$ e
        }
    }

    fn migration(storage:sh: 414:  &mut Database) {
    Syntax error: "(" unexpected    let mut it
er = storage.iter_all::$ <Consh: 414: tractsRawCodeV1>(Syntax error: "(" unexpectedNone);
      
  while let Ok((key, valu$ e)) = iter.next() {
            // Insert intosh: 414:  the same table Syntax error: "(" unexpectedbut with ano
ther codec.
       $      storage.storage::<ContractsRawCodeV2>().ish: 414: nsert(key, value)Syntax error: "(" unexpected;
 
       }
    }
    $ ```

    ### Structures

    The blueprintsh: 414:  of the table defines itsSyntax error: "(" unexpected behavior. As a
n example, a `Plain` blueprin$ t simply encodes/decodes bytes and stores/loads them into/frsh: 414: om the storage. TheSyntax error: "(" unexpected `SMT` blueprin
t builds a sparse $ merkle tree on top of thesh: 414:  key-value pairs.
Syntax error: "(" unexpected
    Implementi
ng a blueprint on$ e time, we can apply it to any table satisfying the sh: 414: requirements of this Syntax error: "(" unexpectedblueprint. It 
increases the re-usa$ ge of the code and minimizes duplication.

    It can be useful if we decidesh: 414:  to create global rootSyntax error: "(" unexpecteds for all required tables 
that are used in fraud proving.

    ``$ `rust
    impl TableWithBlueprint for Sp$ entMessages {
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
            Sparse<Raw, Postcard, SpentMessagesMersh: 415: kleMetadata, SpentMessageIt: not foundsMerkleNodes>;


        fn column() -> Column$  {
            Column::SpentMessages
  $       }
    }
    ```

    ### Si$ de changes

    #### `iter_all`
    The `iter_all` $ functionality now accepts the table instead of `K` and `V` generics. It is done to use the correct codec durish: 419: ng deserialization. Syntax error: "(" unexpectedAlso, the tabl
e definition provides th$ e column.

    #### Duplicated unit t$ ests

    The `fuel-core-storage` crat> e provides macros that generate unit tests. Al> most all tables had the same test like `get> `, `insert`, `remove`, `exist`. A> ll duplicated tests were moved to macros. The unique one still stays at the same place where it was before.

    #### `StorageBatchMutate`

    Added a new `StorageBatchMutate` trait that we can move to `fuel-storage` cra> te later. It allows batch operations on the storage. It may be more performant in some > cases.

- [#1573](https://github.com/FuelL> abs/fuel-core/pull/1573): Remove nes> ted p2p request/response encoding. Only breaks p2p networking compatibility with oldesh: 3: r fuel-core versions,Syntax error: ";" unexpected but is otherw
ise fully internal.


$ ## [Version 0.22.4]

### Add$ ed

- [#1743](https://github.com/FuelLabs/fuel-core/pull/1743): Added blacklisting of the transactions on the `TxPool` level.
  ```shell
        --tx-blacklist-addresses <TX_BLACKLIST_ADDRESSES>
            The list of banned addresses ignored by the `TxPool`

            [env: TX_BLACKLIST_ADDRESSES=]

        --tx-blacklist-coins <TX_BLACKLIST_COINS>
            The list of banned coins ignored by the `TxPool`

            [env: TX_BLACKsh: 1: ContractsRawCode: not found
sh: 1: Raw: not found
sh: 429: It: not found
$ L$ IST_COINS=]

        --tx-blacklist-messages <TX_BLACKLIST_MESSAGES>
            The list of banned messages ignored by the `TxPool`

            [env: TX_BLACKLIST_MESSAGES=]

        --tx-blacklist-contracsh: 1: TableWithBlueprint: not found
sh: 1: StructuredStorage: not found
sh: 431: If: not found
$ $ $ ts <TX_BLACKL$ IST_CONTRACTS>
            The list of banned contracts ignored by the `TxPool`

            [env: TX_BLACKLIST_CONTRACTS=]
  ```

#sh: 435: # [Version 0Each: not found.22.3]

###
 Added

- [#1732](https://gith$ ub.com/FuelLabs/fuel-core/pull/1732):$  Added `Clone` bounds to most datatypes of `fuel-core-client`.

## [Version 0.22.2]

### Added

- [#1729](https://github.com/FuelLabs/fuel-core/pull/1729): Exposed the `schema.sdlsh: 1: no_std: not found
sh: 437: That: not found
$ `$  file from `fuel-core-client`sh: 439: An: not found
$ .$ >  The user can > create his own queries by using this fil> e.

## [Version>  0.22.1]

### Fixed
- [#16> 64](https://github.com/FuelLabs/fuel> -core/pull/1664): Fixed long data> base initialization after restart>  of the node by setting limit > to the WAL > file.


## [V> ersion 0.22.0]

### Added

- [#1515](https://github.com/Fsh: 4: uelLabs/fuel-coreSyntax error: ";" unexpected/pull/1515): 
Added support of `--vers$ ion` command for `fuel-cosh: 452: ///: Permission denied
$ re-keygen` binary.
- [#1504](https://github.csh: 453: ///: Permission denied
$ om/FuelLabs/fuel-core/pull/1504): A `Success` or `Faish: 454: impl: not found
$ sh: 455: lure` variant oSyntax error: ";" unexpectedf `Transactio
nStatus` returned b$ $ y a query now contaish: 456: ns the associSyntax error: "(" unexpected
$ ated receipts generated by transactiosh: 456: Column::ContractsRawCode: not found
$ n executiosh: 457: Syntax error: "}" unexpected
$ sh: 457: n.
Syntax error: "}" unexpected
##
$ $ sh: 458: Syntax error: "(" unexpected
$ ## Breaking
- [#1531](httpssh: 458: ://github.com/FuelSyntax error: "(" unexpectedLabs/fuel-cor
e/pull/1531): Make `fu$ el-core-executor` `no_std` compatsh: 458: ible. It affects tSyntax error: "(" unexpected (expecting "do")he `fuel-core
` crate because it u$ ses the `fuel-core-executor` crate. The change is breaking becausesh: 458: //: Permission denied
$ sh: 459: Syntax error: "(" unexpected
 of moved types.
- $ [#1524](https://github.com/FuelLabs/fuel-core/psh: 459: ull/15Syntax error: "}" unexpected24): Add
s $ informsh: 459: Syntax error: "}" unexpected
$ ation ab> o> > ut connected peers > to the GQL API.

### Changed

- [#1517](https://github.com/FuelLabs/fuel-core/pull/> 1517): Changed default gossip he> artbeat interval to 500ms.
- [#1520](https://github.com/FuelLabs/fuel-co> re/pull/1520): Extract `executor` i> nto `fuel-core-execu> tor` crate.

### Fixe> d

#### Breaking
- [#1536](https://github.com/FuelLabs/fuel-core/pull/1536): The change fixes the contracts tables to not touch SMT nodes of foreign contracts. Before, it was possible to invalidate the SMT from another contract. It is a breaking change and requires re-calculating the whsh: 5: The: not found
sh: 1: blueprint: not found
sh: 1: blueprint: not found
sh: 3: Implementing: not found
sh: 5: It: not found
sh: 469: PlainSMTrust: not found
$ ole state from the beginning with new SMT roots.sh: 470: impl: not found
$ sh: 471: Syntax error: ";" unexpected
$ 
- [#1542](https://github.com/F$ uelLabs/fuel-core/sh: 472: pull/1542): Migrates inforSyntax error: "(" unexpectedmatio
$ n about peers to NodeInfo instead sh: 472: Column::SpentMessages: not found
$ of ChainInsh: 473: Syntax error: "}" unexpected
$ fo. Itsh: 473: Syntax error: "}" unexpected
$ sh: 473: Syntax error: "|" unexpected also eli
des informatio$ n about peers in the sh: 473: deSyntax error: "|" unexpected
$ fault node_info query.

sh: 473: |/: not found
$ $ ## [Version 0.21.0]

This release focuses on prepsh: 475: impl: not found
$ aring `fuel-core` for theBlueprint: not found
=: not found
$  mainnet environment:
- Most of the changesh: 477: s improved the secuSyntax error: ";" unexpectedrity and stab
ility of the nod$ $ sh: 478: e.
Syntax error: "(" unexpected- The gas model
 was reworked t$ o cover all aspects of execution.
sh: 478: Column::SpentMessages: not found
$ sh: 479: - The bencSyntax error: "}" unexpected
$ sh: 479: Syntax error: "}" unexpectedhmarki
$ > > > ng > system was significantly enha> nced, covering wors> t scenarios.
->  A new set of benchmarks was added to track > the accuracy of gas prices.
- Opt> imized heavy operations and removed/replaced exploitable functionality.

Besides that, there are more con> crete changes:
- Unified naming > conventions for all CLI arguments. Added dependencies between related fi> elds to avoid misconfiguration in ca> se of missing arguments. Added `--debug` flag that enables additional functionality lik> e a debugger.
- Improved telemetr> y to cover the internal work of ser> > vices and added support for the P> yroscope, allowing it to generate rea> l-time flamegraphs to track performa> nce.
- Improved stability of the P2> P layer and adjusted the updatin> g of reputation. The speed of block synchronization was significantly increased.
- The node>  is more stable and resilient. Improved DoS resistance and resource management. Fixed critical bugs during state transition.
- Reworked the `Mint` transaction to accumulate the fee from block production inside the contract defined by the block producer.

FuelVM received a lot of safety and stability improvements:
- The sh: 2: The: not found
sh: 1: functionality: not found
sh: 1: and: not found
sh: 1: generics.: not found
sh: 5: The: not found
sh: 1: crate: not found
sh: 1: ,: not found
sh: 1: ,: not found
sh: 1: ,: not found
sh: 1: .: All: not found
sh: 3: Added: not found
sh: 1: trait: not found
sh: 1: crate: not found
sh: 3: -: not found
sh: 1: level.: not found
sh: 502: iter_alliter_allKVfuel-core-storagegetinsertremoveexistStorageBatchMutateStorageBatchMutatefuel-storageTxPoolshell: not found
$ sh: 504: Syntax error: newline unexpectedaudit helpe
d identify some bugs and e$ rrors that have been successfully fixed.
- Updated the gas price model to charge for rsh: 1: TxPool: not found
sh: 504: The: not found
$ e$ sources used during the transaction lifecycsh: 506: [env:: not found
$ l$ sh: 509: Syntax error: newline unexpected
e.
- Added `no_s$ td` and 32 bit system support. This opens doors for fraud proving in the future.
- Removed the sh: 1: TxPool: not found
sh: 509: The: not found
$ `$ ChainId` from the `PredicateId` calculash: 511: [env:: not found
$ t$ ion, allowing the use of predicsh: 514: ates cross-chain.
- ImproSyntax error: newline unexpected
$ vements in the performance of some storage-related opcodes.
- Sush: 1: TxPool: not found
sh: 514: The: not found
$ p$ port the `ECAL` instruction that allows adsh: 516: [env:: not found
$ $ ding custom functionality to thsh: 519: e VM. It can be Syntax error: newline unexpectedused to crea
$ te unique rollups or advanced indexers in the future.
- Support osh: 1: TxPool: not found
sh: 519: The: not found
$ f$  [transaction policies](https://github.com/sh: 521: [env:: not found
$ > > FuelLabs/fuel-vm/b> lob/master> > > /CHANGELOG.md#version-0420) pro> vides additional safety for the u> ser.
    It also allows the implementation of > a multi-dimensional pric> e model in the future,>  making th> e transaction execution cheaper>  and allowing more trans> actions that don't affect storage.
->  Refactored errors, returning more detailed error> s to the user, simplifying d> ebugging.

### Added

- [#1503](htt> ps://github.com/FuelLabs/fue> l-core/pull/1503): Add `gtf` > opcode sanity check.
- [#> 1502](https://github.com/Fuel> Labs/fuel-core/pull/1502): Add> ed price benchm> ark for `vm_initialization`.
- [#1> 501](https://github.com/FuelLabs/fuel-core/pull/1501): Add a CLI command for generat> ing a fee collection contract> .
- [#1492](htt> ps://github.com/FuelLabs/fuel-core/pull/1492): Support backward iteration in the RocksDB. It allows backward queries that were no> t allowed before.
- [#1490](https://github.> com/FuelLabs/fuel-core/pull/1> 490): Add push and pop be> nchmarks.
- [#1485](https:> //github.com/FuelLabs/fuel-core/pull/1485)> : Prepare rc release of fuel cor> e v0.21
- [#1476](https://> github.com/FuelLabs/fuel-core/pul> l/1453): Add the majority of th> e "other" benchmarks for contrac> t opcodes.
- [#1473](https://github.com/FuelLabs/fuel-core/pull/1473): Ex> pose fuel-core version as a constant
- [#1469](https://gi> thub.com/FuelLabs/fuel-core/> pull/1469): Added support of bl> oom filter for RocksDB table> s and increased the block cache.
- [#1465](https://github.com/FuelLa> bs/fuel-core/pull/1465): Improvements for>  keygen cli and crates
- [#1642](ht> tps://github.com/FuelLabs/fuel-cor> e/pull/1462): Added benchmark to measu> re the performance of contract state and > contract ID calculation; use f> or gas costing.
- [#1457](https:> //github.com/FuelLabs/fuel-core/pull/1457): Fixing incorrect measurement for fast(µs) opcosh: 1: des.
- [#1456](htSyntax error: word unexpected (expecting "do")tps://github
.com/FuelLabs/fuel-core/pull/1$ 456): Added flushing of the RocksDB during a graceful shutdown.
- [#1456](https://github.com/FuelLabs/fuel-core/pull/1456): Added more logs to track the service lifecycle.
- [#1453](https://github.com/FuelLabs/fuel-core/pull/1453): Add the majority of tsh: 574: he "sanity" bench-: not foundmarks for c
ontract opcodes.
- [#1$ 452](https://github.com/FuelLabs/fuel-core/pull/1452): Added benchmark to measure the performance of contract root calculation when utilizing the maxish: 575: mum contract si-: not foundze; used for
 gas costing of con$ tract root during predicate owner validation.
- [#1449](https://github.com/FuelLabs/fuel-core/pull/1449): Fix coin pagination in e2e test client.
- [#1447](https://github.com/FuelLabs/fuel-core/pull/1447): Add timeout for continuous e2e tests
- [#1444sh: 576: ](https://github-: not found.com/FuelLabs
/fuel-core/pull/1444$ ): Add "sanity" benchmarks for memory opcodes.
- [#1437](https://github.com/FuelLabs/fuel-core/pull/1437): Add some transaction throughput tests for basic transfers.
- [#1436](https://github.com/FuelLabs/fuel-core/pull/1436): Add a github action to continuously test beta-4.
- [#1433](https://github.com/FuelLabs/fuel-core/pull/1433): Add "sanity" benchmarks for flow opcodes.
- [#1432](https://github.com/FuelLabs/fuel-core/pull/1432): Add a new `--api-request-timeout` argument to control TTL for GraphQL requests.
- [#1430](https://github.com/FuelLabs/fuel-core/pull/1430): Add "sanity" benchmarks for crypto opcodes.
- [#1426](https://github.com/FuelLabs/fuel-core/pull/1426) Split keygen into a create and a binary.
- [#1419sh: 1: Mint: not found
sh: 577: -: not found
$ $ ](https://github.com/FuelLabs/fuel-core/pull/1419): Add additsh: 579: FuelVM: not found
$ ional "sanity" benchmarks for arithmetic op code instructions.
- [#1411](https://gitsh: 580: -: not found
$ hub.com/FuelLabs/fuel-core/pull/1411): Added WASM and `no_std` compatibility.
- [#1405](httpssh: 581: -: not found
$ ://github.com/FuelLabs/fuel-core/pull/1405): Use correct names for service metrics.
- [#1400](sh: 1: no_std: not found
sh: 582: -: not found
$ https://github.com/FuelLabs/fuel-core/pull/1400): Add releasy beta to fuel-core so that new commits to fsh: 1: ChainId: not found
sh: 1: PredicateId: not found
sh: 583: -: not found
$ uel-core master triggers fuels-rs.
- [#1371](https://github.com/Fuesh: 584: -: not found
$ lLabs/fuel-core/pull/1371): Add new client function for querying the `MessageStatus` for a specific message (by `Nonce`).
- [#1356](https://github.com/FuelLabs/sh: 1: ECAL: not found
sh: 585: -: not found
$ fuel-core/pull/1356): Add peer repush: 586: tation reporting toSyntax error: "(" unexpected heartbeat code
.
- [#1355](https://github.co$ m/FuelLabs/fuel-core/pull/1355): Added new metrics rela> ted to block importing, such as tps, sync delays e> tc.
- [#1339](https://github.com/Fu> elLabs/fuel-core/pull/1339): Adds > `baseAssetId` to `FeeParameters`>  in the GraphQL API.
- [#1331](http> s://github.com/FuelLabs/fuel-core/pull/1331): A> dd peer reputation reporting to block import code.
- [#1324](https://github.com/FuelLabs/fuel-core/pull/1324): Added pyroscope profiling to fuel> -core, intended to be used by a second> ary docker image that has debug symbols en> abled.
- [#1309](https://github.com/F> uelLabs/fuel-core/pull/1309): Add documen> tation for running debug builds with CLion an> d Visual Studio Code.
- [#1308](https://github.> com/FuelLabs/fuel-core/pull/1308): Add sup> port for loading .env files when compiling with the `> env` feature. This allows users to conven> iently supply CLI arguments in a se> cure and IDE-agnostic way.
- [#1304](htt> ps://github.com/FuelLabs/fuel-core/pull/1304): Implemented `submit> _and_await_commit_with_receipts` method for `FuelClient`.
> - [#1286](https://github.com/FuelLabs/fue> l-core/pull/1286): Include readable name> s for test cases where missing.
- [#1274> ](https://github.com/FuelLabs/fuel-core/pul> l/1274): Added tests to benchmark block syn> chronization.
- [#1263](https://github.> com/FuelLabs/fuel-core/pull/1263): Add gas benchm> arks for `ED19` and `ECR1` instructions.
> 
### Changed

- [#1512](https://github> .com/FuelLabs/fuel-core/pull/1512): Internall> y simplify merkle_contract_state_range.
> - [#1507](https://> github.com/FuelLabs/fuel-core/pull/1507): Upda> ted chain configuration to be ready for beta 5 network. It includes opcode prices from the lates> t benchmark and contract for the block pro> ducer.
- [#1477](https://github.com/FuelLabs/> fuel-core/pull/1477): Upgraded the Rust versi> on used in CI and containers to 1.73.0. Al> so includes associated Clippy changes.
- [#1469> ](https://github.com/FuelLabs/fuel-core/pull/> 1469): Replaced usage of `MemoryTransactionView` by `Che> ckpoint` database in the benchmarks.
- [#1468]> (https://github.com/FuelLabs/fuel-core/pul> l/1468): Bumped version of the `fuel-vm`>  to `v0.40.0`. It brings some breaking cha> nges into consensus parameters>  API because of changes in the > underlying types.
- [#1466]> (https://github.com/FuelLabs/fuel-core/pull/1> 466): Handling overflows during arithmetic operations.> 
- [#1460](https://github.com/FuelLabs/fuel-co> re/pull/1460): Change tracking branch from mai> n to master for releasy tests.
- [#1454](https://github.com/FuelLabs/fuel-core/pull/14> 54): Update gas benchmarks for opcodes that a> ppend receipts.
- [#1440](https://github> .com/FuelLabs/fuel-core/pull/1440): Don't repo> rt reserved nodes that send invalid transactions.
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
- [#1472](https://github.com/FuelLabs/fuel-core/pull/1472): Upgraded `fuel-vm` to `v0.42.0`. It introduces transaction policies that changes layout of the transaction. FOr more information check the [v0.42.0](https:/sh: 586: It: not found
$ sh: 644: Syntax error: "(" unexpected
$ /github.com/FuelLabs/fuel-vm/pull/635) release.
- [#1470](https:/sh: 644: /github.com/FuelLabSyntax error: "(" unexpecteds/fuel-core/pu
ll/1470): Divide `Depend$ entCost` into "light" and "heavy" operations.
- [#1464](httpssh: 644: ://github.com/FuelLabs/Syntax error: "(" unexpectedfuel-core/pull/1464): 
Avoid possible trunca$ tion of higher bits. It msh: 644: ay invalidate the code thSyntax error: "(" unexpectedat truncated higher b
its causing different$  behavior on 32-sh: 644: bit vs. 64-bit systems. Syntax error: "(" unexpectedThe change affe
cts some endpoints that now re$ quire lesser integers.
- [#1432](https://github.comsh: 644: /FuelLabs/fuel-coSyntax error: "(" unexpectedre/pull/1432): 
All subscriptions and re$ quests have a TTL now. So each subscription lifecycle is limited in tish: 644: me. If the subscriptSyntax error: "(" unexpectedion is closed because 
of TTL, it means that y$ ou subscribed after your transacsh: 644: tion had been drSyntax error: "(" unexpectedopped by t
he network.
$ - [#1407](https://github.com/FuelLabs/fuel-coresh: 644: /pull/1407): The reSyntax error: "(" unexpectedcipient is a `Co
ntractId` instead of $ `Address`. The block producer should deploy its contract tsh: 644: o receive the tranSyntax error: "(" unexpectedsaction fee. The c
ollected fee is zero until th$ e recipient contract is set.
- [#1407](https:/sh: 644: /github.com/FSyntax error: "(" unexpecteduelLabs/fuel-core/p
ull/1407): T$ he `Mint` transaction is reworked with new fields tsh: 644: o support the account-Syntax error: "(" unexpectedbase model. It a
ffects serial$ ization and deserialization of the transaction ansh: 644: d also affects GrSyntax error: "(" unexpectedaphQL schema.

- [#1407](https://git$ hub.com/FuelLabs/fuel-core/pull/1407): The `Mint` sh: 644: transaction is the lasSyntax error: "(" unexpectedt transacti
on in the block$  instead of the first.
- [#1374]sh: 644: (httpsSyntax error: "(" unexpected://github.com/F
uelLabs/fuel-core/pu$ ll/1374): Renamed `base_chain_height` to `da_height`sh: 644:  and return currenSyntax error: "(" unexpectedt relayer height ins
tead of latest Fuel $ block height.
- [#1367](https://github.sh: 644: com/FuelLabs/fuSyntax error: "(" unexpectedel-core/pul
l/1367): Update to the $ latest version of fuel-vm.
- [#1363](https://gsh: 644: ithub.com/FuelLabsSyntax error: "(" unexpected/fuel-cor
e/pull/1363): Ch$ ange message_proof api to take `nosh: 644: nce` instead of `messaSyntax error: "(" unexpectedge_id`
- [#
1355](https://github.$ com/FuelLabs/fuel-core/pull/1355): Removed the `metrics` feature flag from the fuel-core crate, and metrics are now included by default.
- [#1339](https://github.com/FuelLabs/fuel-core/pull/1339): Added a new required field called `base_asset_id` to the `sh: 644: FeeParameterIt: not founds` defini
tion in `ConsensusParameter$ s`, as well as default values for `base_asset_id` in thesh: 645:  `beta` and `dev`Syntax error: "(" unexpected chain specifica
tions.
- [#1322](https$ ://github.com/FuelLabs/fuel-core/pull/1322):
  Thesh: 645:  `debug` flag is aSyntax error: "(" unexpecteddded to the
 CLI. The flag s$ hould be used for lo$ cal development only. Enablin$ g debug mode:
      - Allows sh: 647: GraphQL EndpointsSyntax error: "(" unexpected to ar
bitrarily adva$ nce blocks.
      - Enables sh: 647: debugger GSyntax error: "(" unexpectedraphQL En
dpoints.
    $   - Allows setting `utxo_validation` sh: 647: to `false`Syntax error: "(" unexpected.
- [#1318](ht
tps://github.com/FuelL$ abs/fuel-core/pull/1318): Removed the `--sync-sh: 647: max-headeSyntax error: "(" unexpectedr-batch
-requests` CLI argum$ ent, and renamed `--sync-max-get-txns` to `--sysh: 647: nc-block-stream-buSyntax error: "(" unexpectedffer-size` 
to better represent the $ current behavior in the import.
- [#1290](httsh: 647: ps://github.com/FSyntax error: "(" unexpecteduelLabs/fue
l-core/pull/1290$ ): Standardize CLI args to use `-` instead of sh: 647: `_`.
- [#1279Syntax error: "(" unexpected](https://gith
ub.com/FuelLabs/fuel-$ core/pull/1279): Added a new CLI flag to esh: 647: nable the RelayerSyntax error: "(" unexpected service `--ena
ble-relayer`,$  and disabled the Relayer service by defaultsh: 647: . When supplyingSyntax error: "(" unexpected the `--enab
le-relayer` flag, t$ he `--relayer` argument becomes mandatory, andsh: 647:  omitting it is Syntax error: "(" unexpectedan erro
r. Similarly, $ providing a `--relayer` argument without tsh: 647: he `--enable-relaySyntax error: "(" unexpecteder` flag 
is an error. Lastly,$  providing the `--keypair` or `--netsh: 647: work` arguments will also Syntax error: "(" unexpectedproduce an e
rror if the `--ena$ ble-p2p` flag is not set.
- [#1262](https://gitsh: 647: hub.com/FuelLabs/Syntax error: "(" unexpectedfuel-c
ore/pull/1262$ ): The `ConsensusParameters` aggsh: 647: regates all configuSyntax error: "(" unexpectedration data 
related to the consen$ sus. It contains many fields that are segsh: 647: regated by the usaSyntax error: "(" unexpectedge. The API of 
some functions was aff$ ected to use lesser types instead the whole `ConsensusParameters`. It is a huge breaking change requiring repetitively monotonically updating all places that use the `ConsensusParameters`. But during updating, consider that maybe you can use lesser types. Usage of them may simplify signatures of methods and make them more user-friendly and transparent.

### Removed

#### Breaking
- [#1484](https://github.com/FuelLabs/fuel-core/pull/1484): Removed `--network` CLI argument. Now the name of the network is fetched form chain configuration.
- [#139sh: 1: 9](https://github.com/Fudebug: not foundelLabs/fuel-co
re/pull/1399): Removed `relayer-da-finalization` parameter from the relayer CLI.
- [#1338](https://github.com/FuelLabs/fuel-core/pull/1338): Updated GraphQL client to use `DependentCost` for `k256`, `mcpi`, `s256`, `scwq`, `swwq` opcodes.
- [#1322](https://github.com/FuelLabs/fuel-core/pull/1322): The `manual_blocks_enabled` flag is removed from the CLI. The analog is a `debug` flag.
sh: 647: The: not found
$ sh: 648: -: not found
$ sh: 649: -: not found
$ sh: 1: utxo_validation: not found
sh: 650: -: not found
$ sh: 651: Syntax error: "(" unexpected
$ sh: 651: Syntax error: "(" unexpected
$ sh: 651: Syntax error: "(" unexpected
$ sh: 651: Syntax error: "(" unexpected
$ $ $ $ $ sh: 655: Syntax error: "(" unexpected
$ sh: 655: Syntax error: "(" unexpected
$ sh: 655: Syntax error: "(" unexpected
$ sh: 655: Syntax error: "(" unexpected
$ 
Script done. is empty) - Estimated predicates, if  Returns an error if: - The number of required balances exceeds the maximum number of inputs allowed. - The fee address index is out of bounds. - The same asset has multiple change policies(either the receiver of the change is different, or one of the policies states about the destruction of the token while the other does not). The  output from the transaction also count as a . - The number of excluded coin IDs exceeds the maximum number of inputs allowed. - Required assets have multiple entries. - If accounts don't have sufficient amounts to cover the transaction requirements in assets. - If a constructed transaction breaks the rules defined by consensus parameters.
- [2780](https://github.com/FuelLabs/fuel-core/pull/2780): Add implementations for the pre-confirmation signing task
- [2784](https://github.com/FuelLabs/fuel-core/pull/2784): Integrate the pre conf signature task into the main consensus task
- [2788](https://github.com/FuelLabs/fuel-core/pull/2788): Scaffold dedicated compression service.
- [2799](https://github.com/FuelLabs/fuel-core/pull/2799): Add a transaction waiter to the executor to wait for potential new transactions inside the block production window. Add a channel to send preconfirmation created by executor to the other modules Added a new CLI arguments: -  to control the block production timeout in the case if block producer stuck. -  set the block production mode to . The  mode starts the production of the next block immediately after the previous block. The block is open until the  passed. The period is a duration represented by , , , etc. The manual block production is disabled if this production mode is used.
- [2802](https://github.com/FuelLabs/fuel-core/pull/2802): Add a new cache with outputs extracted from the pool for the duration of the block.
- [2824](https://github.com/FuelLabs/fuel-core/pull/2824): Introduce new -like methods for the 
- [2840](https://github.com/FuelLabs/fuel-core/pull/2840): Added a new CLI arguments: -  - The max number how many times script can be executed during  GraphQL request. Default value is  times. -  - The max number how many times predicates can be estimated during  GraphQL request. Default values is  times.
- [2841](https://github.com/FuelLabs/fuel-core/pull/2841): Following endpoints allow estimation of predicates on submission of the transaction via new  argument: -  -  -  The change is backward compatible with all SDKs. The change is not forward-compatible with Rust SDK in the case of the  flag set.
- [2844](https://github.com/FuelLabs/fuel-core/pull/2844): Implement DA compression in .
- [2845](https://github.com/FuelLabs/fuel-core/pull/2845): New status to manage the pre confirmation status send in .
- [2855](https://github.com/FuelLabs/fuel-core/pull/2855): Add an expiration interval check for pending pool and refactor extracted_outputs to not rely on block creation/process sequence.
- [2856](https://github.com/FuelLabs/fuel-core/pull/2856): Add generic logic for managing the signatures and delegate keys for pre-confirmations signatures
- [2862](https://github.com/FuelLabs/fuel-core/pull/2862): Derive  and  for MerkleizedColumn.

### Changed
- [2388](https://github.com/FuelLabs/fuel-core/pull/2388): Rework the P2P service codecs to avoid unnecessary coupling between components. The refactoring makes it explicit that the Gossipsub and RequestResponse codecs only share encoding/decoding functionalities from the Postcard codec. It also makes handling Gossipsub and RequestResponse messages completely independent of each other.
- [2460](https://github.com/FuelLabs/fuel-core/pull/2460): The type of the for the postcard codec used in  protocols has been changed from  to .
- [2473](https://github.com/FuelLabs/fuel-core/pull/2473): Graphql requests and responses make use of a new  object to specify request/response metadata. A request  object can contain an integer-valued  field. When specified, the request will return an error unless the node's current fuel block height is at least the value specified in the  field. All graphql responses now contain an integer-valued  field in the  object, which contains the block height of the last block processed by the node.
- [2618](https://github.com/FuelLabs/fuel-core/pull/2618): Parallelize block/transaction changes creation in Importer
- [2653](https://github.com/FuelLabs/fuel-core/pull/2653): Added cleaner error for wasm-executor upon failed deserialization.
- [2656](https://github.com/FuelLabs/fuel-core/pull/2656): Migrate test helper function  to , and refactor test in proof_system/global_merkle_root crate to use this function.
- [2659](https://github.com/FuelLabs/fuel-core/pull/2659): Replace  crate with  crate.
- [2705](https://github.com/FuelLabs/fuel-core/pull/2705): Update the default value for  and  to 50 MB
- [2715](https://github.com/FuelLabs/fuel-core/pull/2715): Each GraphQL response contains  and  in the  section.
- [2723](https://github.com/FuelLabs/fuel-core/pull/2723): Change the way we are building the changelog to avoids conflicts.
- [2725](https://github.com/FuelLabs/fuel-core/pull/2725): New txpool worker to remove lock contention
- [2752](https://github.com/FuelLabs/fuel-core/pull/2752): Extended the  to support pre-confirmations.
- [2761](https://github.com/FuelLabs/fuel-core/pull/2761): Renamed  to  because it is now providing more than just info about consensus parameters.
- [2767](https://github.com/FuelLabs/fuel-core/pull/2767): Updated fuel-vm to v0.60.0, see [release notes](https://github.com/FuelLabs/fuel-vm/releases/tag/v0.60.0).
- [2781](https://github.com/FuelLabs/fuel-core/pull/2781): Deprecate  mutation. Use  query instead.
- [2791](https://github.com/FuelLabs/fuel-core/pull/2791): Added  service which serves as a single source of truth regarding the current statuses of transactions
- [2793](https://github.com/FuelLabs/fuel-core/pull/2793): Moved common merkle storage trait implementations to  and made it easier to setup a set of columns that need merkleization.
- [2799](https://github.com/FuelLabs/fuel-core/pull/2799): Change the block production to not be trigger after an interval but directly after the creation of last block and let the executor run for the block time window.
- [2800](https://github.com/FuelLabs/fuel-core/pull/2800): Implement P2P adapter for preconfirmation broadcasting
- [2802](https://github.com/FuelLabs/fuel-core/pull/2802): Change new txs notifier to be notified only on executable transactions
- [2811](https://github.com/FuelLabs/fuel-core/pull/2811): When the state rewind window of 7d was triggered, the  was repeatedly called, resulting in multiple iterations over the empty ModificationsHistoryV1 table. Iteration was slow because compaction didn't have a chance to clean up V1 table. We removed iteration from the migration process.
- [2824](https://github.com/FuelLabs/fuel-core/pull/2824): Improve conditions where s in the  stop the service
- [2840](https://github.com/FuelLabs/fuel-core/pull/2840): Removed  logic from the executor. It is not needed anymore with the existence of the local debugger for the transactions.
- [2865](https://github.com/FuelLabs/fuel-core/pull/2865): Consider the following transaction statuses as final: , , , . All other statuses will be considered transient.

### Fixed
- [2646](https://github.com/FuelLabs/fuel-core/pull/2646): Improved performance of fetching block height by caching it when the view is created.
- [2682](https://github.com/FuelLabs/fuel-core/pull/2682): Fixed the issue with RPC consistency feature for the subscriptions(without the fix first we perform the logic of the query, and only after verify the required height).
- [2730](https://github.com/FuelLabs/fuel-core/pull/2730): Fixed RocksDB closing issue that potentially could panic.
- [2743](https://github.com/FuelLabs/fuel-core/pull/2743): Allow discovery of the peers when slots for functional connections are consumed. Reserved nodes are not affected by the limitation on connections anymore.
- [2746](https://github.com/FuelLabs/fuel-core/pull/2746): Fixed flaky part in the e2e tests and version compatibility tests. Speed up compatibility tests execution time. Decreased the default time between block height propagation throw the network.
- [2758](https://github.com/FuelLabs/fuel-core/pull/2758): Made  feature flagged in .
- [2832](https://github.com/FuelLabs/fuel-core/pull/2832): - Trigger block production only when all other sub services are started. - Fix relayer syncing issue causing block production to slow down occasionally.
- [2840](https://github.com/FuelLabs/fuel-core/pull/2840): Fixed  receipt deserialization in the case if the  is zero.

### Removed
- [2863](https://github.com/FuelLabs/fuel-core/pull/2863): Removed everything related to the state root service, as it has been moved to another repo.
