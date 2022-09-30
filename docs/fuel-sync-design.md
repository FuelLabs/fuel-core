# The `fuel-sync` design the version for PoA

## Changelog

- Created: 28.09.2022

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

The file describes the workflow of the FS service and its relationship with P2P, CM and BI.
During the review process, the team selected which way to go, and the 
document's final version contains the selected way with a detailed overview 
and rejected ways for history.

The final version of the document describes the implementation
of the FS for the PoA version of the FC. It also contains our assumptions
and ideas for the next evolution of the FS for PoS.

All types mentioned in the design only describe their primary goal. 
The implementation should use the type that fits better. For example, 
instead of slices of objects, better works vectors because the caller 
should own data.

## Relationship with P2P and BI

![img.png](diagrams/fuel-core-design-fuel-sync-part.png)

The FS is a connection point between P2P, CM and BI. FS manages receiving 
blocks from P2P, validation of them with CM, and forwarding them to BI 
for commit.

Fs use the following rule to manage relationship:

> Every service **may** know about descending services but nothing about
ascending services. As an example: P2P service knows nothing about its
listeners(ascending services), and it only propagates information via
channels. But FS knows about P2P(an example of descending service)
and may request some information directly by calling the sync/async methods.
The same applies to BI and CM.

This rule is rough, and services can have cases where some functionality
requires direct access and some pub-sub model. So it should be decided
individually. But after comparison and discussion, this rule covers all our needs
for now, and the description below will use this rule. We can stop
following this rule if it makes development harder, adds some complexity,
or forces us to write ugly code with workarounds.

FS - is ascending service for P2P, BI, CM.

Other rejected by the team possible rules of the services' relationship:

1. Every service knows about other services related somehow to it. It 
can be interpreted as: each service struct containing reference to other 
services inside and using the sync/async mechanism to interact.
1. Every service knows nothing about other services. It can be interpreted 
as: each service struct using channels for communication and knowing 
nothing about subscribers/listeners.

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
    HeadersByRange(Range<BlockHeight>),
    HeadersByHash([Bytes32]),
    BlocksByRanges([Range<BlockHeight>]),
    BlocksByHash([Bytes32]),
    /// Ask neighbors about the latest block height
    Heartbeat,
}

pub enum Gossip {
  Transaction(Transaction),
  Block(Sealed<Block>)
}

pub trait Gossiper {
  /// Gossiping a `data` to the network.
  fn gossiping(&self, data: &Gossip);
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
    /// Subscribes for `BlockBroadcast` events.
    fn subscribe(&self) -> Receiver<BlockBroadcast>;

    /// Fetches the data(somehow, maybe in parallel) and broadcasts ready parts.
    fn fetch(&mut self, request: FetchRequest);
}
```

The FS stores the reference(or a wrapper structure around P2P to allow `mut` 
methods) to the P2P and interacts with it periodically. 
All data is received from the channel and processed by FS to update the 
inner state of the synchronization. FS requests data from P2P for synchronization each time when it is required. 
If some data is invalid or corrupted, the FS reports 
it to P2P, and P2P can punish the sender of the data by decreasing the 
reputation(or blacklisting).

The same approach can be used between the P2P and TP. 
`Punisher` trait adds a mechanism for [reputation management](https://github.com/FuelLabs/fuel-core/issues/460).
`Gossiper` trait solves [gossiping of invalid transactions](https://github.com/FuelLabs/fuel-core/issues/595).


### BI requirements

The FS is ascending service for the BI because BI knows nothing about 
other services; it only executes blocks and updates the state of the 
blockchain. At the end of the block commit, it notifies subscribers 
about the block and calls `Gossiper::gossiping`.

```rust
pub enum CommitBroadcast {
    Blocks([Sealed<FuelBlock>]),
}

pub trait BlocksCommitter {
    /// Subscribes for `CommitBroadcast` event.
    fn subscribe(&self) -> Receiver<CommitBroadcast>;

    /// Commits the blocks to the database. Executes each block from 
    /// `blocks`, validates it, and updates the state.
    /// 
    /// Return errors if something is wrong during the commit process.
    /// The state is not updated in that case.
    async fn commit(&mut self, blocks: [Sealed<FuelBlock>]) -> Result<(), CommitError>;

    /// Returns the last committed block header.
    fn last_block_header(&self) -> FuelBlockHeader;
}
```

The FS can use the result of the `commit` function to update the inner state 
of the synchronization and report in case of an error. The `subscribe` method 
is helpful for a TP to prune committed transactions.

### CM requirements

The CM is descending service for the FS and used only to validate blocks.
CM knows all information about the current consensus and can do this validation.

```rust
pub trait SealValidator {
    /// Validates that the seals of the block are correct according to the consensus rules.
    async fn validate(&mut self, blocks: [Sealed<FuelBlock>]) -> Verdict<BlockReason>;
}
```

If CM can't validate information because the Relayer is not synced enough, 
it awaits synchronization of the Relayer if the blocks are not far away.

The inner logic looks like:

```mermaid
flowchart
    a["if block.da_height > relayer.da_height"] -- no --> b["Validate seals"]
    b -- valid --> s["Verdict<BlockReason>::Positive"]
    b -- invalid --> f["Verdict<BlockReason>::Negative"]
    a -- yes --> c["awaits synchronization of the Relayer"]
    c --> d["if block.da_height > relayer.da_height + CONST(adjustable margin)"]
    d -- yes --> f
    d -- no --> a
```

After validation of seals, FS forwards block to BI.

### Remark

The actual implementation may have different types but with the same 
meaning. If the P2P, CM and BI provide described functionality, it is enough 
to implement FS and connect services fully.

## The blocks synchronization flow

The start point of synchronization is a sealed block or block header. 
Not sealed blocks are part of the consensus, and P2P forwards them to 
the consensus subscription channel.

### The block propagation in the network

We have three possible ways of gossiping(about new block):

1. Gossip the height and block hash of a new block.
1. Gossip the header of a new block with the producer's seal(or consensus seal).
1. Gossip the entire block.

The first and second approaches require an additional request/response 
round to get the block. But the network passive resource consumption is 
low. With the first approach, malicious peers can report unreal heights, 
and FS needs to handle those cases in the code. With the second case, FS 
can verify the block's validity and start the synchronization process 
only in the case of validity.

The third approach provides the entire block, and it can be a faster 
way to keep the network updated. But it will spam the web with oversized 
useless packages(if each node has ten connections, then nine of the 
received packages are useless).

We already need to support syncing with requests/responses in the 
code for outdated nodes. So, using the third approach doesn't win much 
regarding the codebase size.

The second approach is our choice for PoA. When FS receives the sealed block header, 
it can already be sure this is not a fake sync process. The passive load for 
the network is low.

But it is not problem to try to sync to not valid height. P2P will 
request blocks for that height from the peer who reported it, and it will 
decrease the reputation. So we can use an approach with heights too. It requires
proper handling of this case.

#### Node behaviour

All nodes have the same FS, so it is enough to describe the general work of FS.

##### Gossip about new data

Each node gossips only information that is validated by itself and can be 
proved by itself. It means the node gossips the block header only after 
committing the block.

The final destination of all blocks is BI, so BI is responsible for 
calling `Gossiper::gossiping` of P2P. It gossips the blocks received 
from the network and from the consensus(when the node is block producer).

##### Ask about new data

The network gossips a new block by default with an `ABGT` interval with 
the [rule above](#gossip-about-new-data). But it is possible to have cases where 
someone forgot to notify another node.

FS has an internal timer that pings the P2P every `ABGT + Const`
(for example, `Const = 5` seconds). This timer resets on each block 
commit or timer tick. P2P, on each ping, asks neighbors about the latest block header.

### The blocks synchronization on block header event

FS has two synchronization phases, one is active, and another is pending. 
Each phase has its range of blocks to sync. They don't overlap and are 
linked. The purpose of the pending phase is to collect and join new 
block headers while the active phase syncs the previous range. When 
the first phase finishes synchronization, it takes some range from the 
pending phase, joins with the remaining range from the active phase, 
and requests blocks for the final range.

Because each block header is sealed, and FS knows the block producer 
of each block, it allows us to verify the validity of the header 
and be sure that the block range is actual without fake heights.

#### Fork

If FS gets the valid sealed block header but already have another block for this height, it is a case of the fork.
- For the PoA version: We ignore it and try to find blocks that link with our version of the blockchain. We should report it to the node manager(via logs, or maybe we want to do something more).
- For the PoS version: We should have rules in the consensus for this case.

#### Queuing of the block header

When FS receives a block header, first FS checks:
- Is it a valid header? Blocks should be linked, and signatures should match.
After, forwards blocks to the CM and awaits verification of producer/consensus
at the according height.
  - No -> `Punisher::report`.
  - Yes -> Next step.
- Is it a new height?
  - No -> Is it already in the blockchain?
    - Yes -> `Punisher::report` with the "duplicate" reason. P2P decides what to do with duplicating packages.
    - No -> It is case of the [fork](#fork).
  - Yes -> Next step.
- Is synchronization in progress?
  - No -> Create an active phase and init it. Start synchronization process.
  - Yes -> Next step.
- Is this height already handled by the active phase?
  - Yes -> If the header is the same?
    - Yes -> `Punisher::report` with the "duplicate" reason. P2P decides what to do with duplicating packages.
    - No -> It is case of the [fork](#fork).
    - Don't know because it is in the middle of the range ->
      FS inserts this header into mapping and will check it later, 
      when blocks will come.
  - No -> Insert the header into range of the pending phase.

Each valid modification of the active or pending phase triggers requesting 
blocks for the active phase from P2P. P2P is responsible for managing how 
to process those requests. P2P decides which peers to ask, manages timeouts, 
and manages duplicate requests from other services.

P2P is also responsible for punishing nodes that send not requested information. 
Other nodes can only share information about their last block header(But it also 
should be limited because each node applies the block only once).

The P2P's responsibility can be part of the synchronizer instead, and it can store 
the information about the height of the peers and has an internal 
reputation who ask about blocks. It will make FS contain the full 
logic about synchronization but will duplicate reputation.

#### The active phase

Active phase has a range of currently syncing blocks. Each time FS 
receives a block header, it stores this header in a separate mapping. 
It is optional for FS because it only helps find forks. When FS receives 
the block for corresponding height, it compares headers.

If the range is extensive, FS splits it into batches, and only the first batch is
a part of the active phase, the remaining range is part of the pending phase.
FS requests only one batch simultaneously from P2P. It is possible to 
request several batches in parallel in the future.

When blocks come, it checks that they are valid, linked, and adequately 
sealed(the check is done via CM). If all checks pass, FS forward the blocks 
to BI and starts [commit process](#the-commit-of-the-blocks). 
If blocks are not valid, FS reports invalidity to P2P and requests block range again.


### The commit of the blocks

When FS receives blocks linked to the current latest block of the 
blockchain, FS starts to commit them(`BlockCommiter::commit`). Only one 
commit can be run in the BI at the exact moment, so BI has local mutex 
only for `commit` to prevent multiple commits.

`BlockCommiter::commit` return the result of execution:
- In the case of successful execution. FS removes committed blocks from 
the range of active phase. After FS fulfills it with a new range from 
the pending phase. The new range should always be <= the batch size. 
FS requests a new fulfilled range from P2P.
- It can be an error that those blocks already are committed. It is a 
case of [fork](#fork), but it is still good because another job has already 
committed blocks. Do the same as in the case of successful execution.
- It can be an error that BI got some error during execution. It is a
bad case, and it means that FS got a valid block signed by the producer, 
but FS can't apply it to the blockchain. Report it to logs and notify the node owner.
