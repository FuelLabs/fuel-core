# The `fuel-sync` design the version for PoA

## Changelog

- TODO: Date of PR creation/approval

## Glossary

- FS - Fuel sync -> `fuel-sync`
- P2P - Fuel p2p -> `fuel-p2p`
- BI - Fuel block importer -> `fuel-block-importer`
- FC - Fuel core -> `fuel-core`
- CM - Consensus module
- Sealed data - The data(blocks, transactions, headers, etc.) signed by the block producer(PoA) or by the consensus comity(PoS)
- ABGT - Average block generation time in seconds
- TP - Transactions pool -> `fuel-txpool`

## Overview

The file describes the possible workflow of the FS service 
and its relationship with P2P and BI.
During the review process, we must select which way to go, and the document's 
final version will contain only the selected way with a proper overview.

The first final version of the document will describe the implementation 
of the FS for the PoA version of the FC. It may contain our assumptions 
and ideas for the next evolution of the FS for PoS(or maybe we will 
have several evolution points).

All names are not final and can be suggested by the reviewers=)

## Relationship with P2P and BI

![img.png](diagrams/fuel-core-design-fuel-sync-part.png)

The FS is a connection point between:
- In the PoA version P2P and BI
- In the PoS is also additionally connects P2P with CM

There are three possible ways how to implement those relationships:

1. Every service knows about other services related somehow to it. It 
can be interpreted as: each service struct containing reference to other 
services inside and using the `async/await` mechanism to interact.
1. Every service knows nothing about other services. It can be interpreted 
as: each service struct using channels for communication and knowing 
nothing about subscribers/listeners.
1. Every service **may** know about descending services but nothing about 
ascending services. As an example: P2P service knows nothing about its 
listeners(ascending services), and it only propagates information via 
channels. But FS knows about P2P(an example of descending service)
and may request some information directly by calling the `async` method. 
The same applies to BI. FS notifies all subscribers that it synced a valid block(sealed or not sealed).

Those rules are rough, and we can have cases where some functionality 
requires direct access and some pub-sub model. So it should be decided 
individually. But after comparison, the third rule covers all our needs 
for now, and the description below will use this rule. We can stop 
following this rule if it makes development harder, adds some complexity, 
or forces us to write ugly code with workarounds.

### P2P service requirements

The P2P service should not be aware of FS but still should provide some 
functionality that the FS can use. As a solution, P2P can implement the 
`BlocksFetcher` trait.

```rust
pub enum BlockBroadcast {
    NewSealedHeaders([Sealed<FuelBlockHeader>]),
    NewUnsealedHeaders([FuelBlockHeader]),
    NewSealedBlocks([Sealed<FuelBlock>]),
    NewUnsealedBlocks([FuelBlock]),
    // TODO: Some PoS related events
}

pub enum BlockReason {
    InvalidHeader,
    InvalidBody,
    // TODO: ?
}

pub enum Verdict<Reason> {
    Positive,
    Negative(Reason),
}

pub enum FetchRequest {
    HeadersByRanges([Range<BlockHeight>]),
    HeadersByHash([Bytes32]),
    BlocksByRanges([Range<BlockHeight>]),
    BlocksByHash([Bytes32]),
}

pub trait Gossiper {
    /// Gossips some `data` to the network.
    // TODO: `D` should implement some bound that allows to serialize it for gossiping.
    fn gossip<D>(&self, data: &D);
}

pub trait Punisher<Reason> {
    /// Anyone can report the validity or invalidity of the data. The `Punisher` decide how to process it.
    ///
    /// Example: If the transaction or blocker header is invalid, the punisher can decrease the reputation 
    /// of the data provider.
    // TODO: `D` should implement some bound that allows retrieving the data's signer and allowing to broadcast it.
    fn report<D>(&mut self, data: D, verdict: Verdict<Reason>);
}

pub trait BlocksFetcher: Punisher<BlockReason> {
    /// Returns structure that can be used to subscribe for `BlockBroadcast`.
    fn sender(&self) -> &Sender<BlockBroadcast>;

    /// Pings the block fetcher to fetch a new latest block header and broadcasts it.
    fn ping(&mut self);

    /// Fetches the data(somehow, maybe in parallel) and broadcasts ready parts.
    fn fetch(&mut self, request: FetchRequest);
}
```

The FS stores the reference(or a wrapper structure around P2P to allow `mut` 
methods) to the P2P and interacts with it periodically. 
All data is received from the channel and processed by FS to update the 
inner state of the synchronization. If FS requires more data to finish the synchronization, 
we will request it. If some data is invalid or corrupted, the FS reports 
it to P2P, and P2P can punish the sender of the data by decreasing the 
reputation(or blacklisting). FS calls `Gossiper::gossip` at the end of block synchronization.

The same approach can be used between the P2P and TP. 
`Punisher` trait adds a mechanism for [reputation management](https://github.com/FuelLabs/fuel-core/issues/460).
`Gossiper` trait solves [gossiping of invalid transactions](https://github.com/FuelLabs/fuel-core/issues/595).


### BI requirements

The FS is ascending service for the BI because BI knows nothing about 
other services; it only executes blocks and updates the state of the 
blockchain. At the end of the block commit, it notifies subscribers.

```rust
pub enum CommitBroadcast {
    Blocks([FuelBlock]),
}

pub trait BlocksCommitter {
    /// Returns structure that can be used to subscribe for `CommitBroadcast`.
    fn sender(&self) -> &Sender<CommitBroadcast>;

    /// Commits the blocks to the database. Executes each block from 
    /// `blocks`, validates it, and updates the state.
    /// 
    /// Return errors if something is wrong during the commit process.
    /// The state is not updated in that case.
    fn commit(&mut self, blocks: [FuelBlock]) -> Result<(), CommitError>;

    /// Returns the last committed block header.
    fn last_block_header(&self) -> FuelBlockHeader;
}
```

FS needs to subscribe to committed blocks because blocks can be generated 
by the block production service(or consensus). The FS can use the result 
of the `commit` function to update the inner state of the synchronization 
in case of an error. Also, `sender` method is helpful for a TP 
to prune committed transactions.

### Remark

The actual implementation may have different types but with the same 
meaning. If the P2P and BI provide described functionality, it is enough 
to implement FS and connect services fully.

## The blocks synchronization flow

FS has two types of synchronization:
- During PoS consensus. It includes validation of blocks, transactions, 
and signatures. A partially sealed(not all participants of consensus 
signed it) block may be the starting point of the synchronization.
- During sync with the network. The start point is a sealed block or 
block header.

The section contains the description of the flow for the second type. 

### The block propagation in the network

We have three ways of gossiping(about new block) implementation:

1. Gossip only the height of a new block.
1. Gossip the header of a new block with the producer's seal(or consensus seal).
1. Gossip the entire block.

The first and second approaches require an additional request/response 
round to get the block. But the network passive resource consumption is 
low. With the first approach, malicious peers can report unreal heights, 
and we need to handle those cases in the code. With the second case, we 
can verify the block's validity and start the synchronization process 
only in the case of validity.

The third approach provides the entire block, and it can be a faster 
way to keep the network updated. But it will spam the web with oversized 
useless packages(if each node has ten connections, then nine of the 
received packages are useless).

We already need to support syncing with requests/responses in the 
code for outdated nodes. So, using the third approach doesn't win much 
regarding the codebase size.

The second approach is our choice. When we receive the sealed block header, 
we can already be sure it is not a fake sync process. The passive load for 
the network is low.

#### Node behaviour

All nodes have the same FS, so it is enough to describe the work of FS.

##### Gossip about new data

Each node gossips only information that is validated by itself and can be 
proved by itself. It means the node gossips the block header only after 
committing the block.

The final destination of all blocks is BI. FS subscribes to events from 
the BI service and gossips the information about the latest block to 
the network on each commit via the `Gossiper` trait implemented by P2P.

It gossips the blocks received from the network, from the block producer, and from the consensus.

##### Ask about new data

The network gossips a new block by default with an `ABGT` interval with 
the [rule above](#gossip-about-new-data). But we can have cases where 
someone forgot to notify another node.

FS has an internal timer that pings the P2P every `ABGT + Const`
(for example, `Const = 5` seconds). This timer resets on each block 
commit or timer tick. P2P, on each ping, asks neighbors about the latest block header.

### The blocks synchronization on block header event

TODO: Rework this section to support cases where we can change the block producer

FS has two synchronization structs, one is active, and another is pending. 
Each struct has its range of blocks to sync. They don't overlap and are 
linked. The purpose of the pending struct is to collect and join new 
block headers while the active struct syncs the previous range. When 
the first struct finishes synchronization, it swaps with the second 
struct, and the process repeats.

Because each block header is sealed, and we know the block producer 
of each block, it allows us to verify the validity of the header 
and be sure that the block range is actual without fake heights.

#### Fork

If we get the valid sealed block header but already have another block for this height, it is a case of the fork.
- For the PoA version: We ignore it and try to find blocks that link with our version of the blockchain. We should report it to the node manager(via logs, or maybe we want to do something more).
- For the PoS version: We should have rules in the consensus for this case.

#### Queuing of the block header

When FS receives a block header, first we check:
- Is it a valid header? It should be signed by the expected, for the node, producer/consensus at according height.
  - No -> `Punisher::report`.
  - Yes -> Next step.
  - Not sure. Not sure. It is possible in the cases when we change the 
  block producer/consensus. The node on the current height is sure 
  about the block producer of the next block but can't predict it.
- Is it a new height?
  - No -> Ignore (Maybe report some neutral error to P2P to prevent DDoS) if it is already in the blockchain, either go to [fork](#fork).
  - Yes -> Next step.

TODO: Rework this part


#### The active struct

TODO: Rework this part

#### The pending struct

TODO: Rework this part

### The commit of the blocks

When FS receives blocks linked to the current latest block of the 
blockchain, we start to commit them(`BlockCommiter::commit`). Only one 
commit can be run in the BI at the exact moment, so BI has local mutex 
only for `commit` to prevent multiple commits.

`BlockCommiter::commit` return the result of execution:
- In the case of successful execution, we mark the job finished and 
remove it. We wait for the event from the channel to clean up other jobs 
related to the same height.
- It can be an error that those blocks already are committed. It is a 
good case because another job has already committed blocks. Do the same 
as in the case of successful execution.
- It can be an error that BI got some error during execution. It is a 
bad case. Call `Punisher::report` to report an invalid block. If we get
invalid blocks from many other peers, maybe we need to report this information 
to the node owner via logs(perhaps something is wrong with the node's 
software).

#### On block commit event

FS gossips about the node's latest via `Gossiper::gossip`.

FS clean-ups all synchronization jobs where the maximum height of the 
range is less or equal to the maximum committed height.