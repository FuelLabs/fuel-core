# Post-Mortem: Why the FizzBee Model Missed Two Critical Bugs

## Executive Summary

The original FizzBee specification (`sequencer_ha.fizz`) exhaustively verified 58,409 states and reported no safety violations. However, chaos testing of the real implementation discovered two critical failures:

1. **Fork** (safety violation): Two sequential leaders both achieved quorum writes at the same block height, producing two different committed blocks — a chain fork.
2. **Livelock** (liveness violation): Orphaned sub-quorum entries in Redis permanently blocked new leaders from producing blocks at that height.

Both bugs were caused by **abstraction errors** in the formal model — places where the model's simplifications silently eliminated the failure modes that existed in the real system. This document catalogs these errors and explains the corrected model (`sequencer_ha_v2.fizz`).

## Bug 1: Fork via Missing HEIGHT_EXISTS

### What Happened

Chaos test seed 8 produced a fork at height 326. Redis stream dump at the moment of fork:

```
Redis 0: height=326 epoch=295
Redis 1: height=326 epoch=292
Redis 2: height=326 epoch=295 AND epoch=292 (two entries!)
```

Leader A (epoch 292) wrote a block at height 326 to Redis 1. Leader B (epoch 295) wrote a *different* block at height 326 to Redis 0 and Redis 2. Both achieved quorum (2/3). Two different blocks at the same height, both quorum-committed. Fork.

### Why the Model Missed It

**Flaw 1 — No HEIGHT_EXISTS check in `write_block`**: The model's `write_block` function (lines 76-90 of the original spec) performs identity and fencing checks but always appends to the stream unconditionally. There is no check for whether an entry at the requested height already exists. This matches the *original* `write_block.lua` implementation, which also lacked this check — the model accurately reflected a buggy design, but nobody recognized the bug because the model "passed."

**Flaw 2 — Atomic commit fallacy**: The `ProduceBlock` action (lines 169-196) models the entire sequence — Redis quorum write, success check, and local state advance (`self.next_height = height + 1`) — as a single `atomic action`. In reality, these are separate operations with a yield point between the Redis publish and the local commit. A crash or lock expiry between these steps is a critical interleaving the model never explored.

Because `ProduceBlock` is atomic, the model checker never sees the state where: (a) the Redis quorum write succeeded, (b) the leader has not yet advanced its local state, and (c) the lock expires and a new leader promotes. In this window, the new leader reads the stream, may not see the old leader's block on quorum (due to partial write or read failure), and produces its own block at the same height.

### The Fix

1. **HEIGHT_EXISTS**: `write_block.lua` now scans the stream for existing entries at the requested height before `XADD`. If found, the write is rejected unconditionally. Combined with the pigeonhole principle (any two quorums of size `ceil(N/2)+1` overlap), this guarantees no two different blocks at the same height can both achieve quorum.

2. **De-atomized ProduceBlock**: The new spec splits `ProduceBlock` into two actions — the Redis quorum write phase and a separate `CommitLocal` action — with an implicit yield point between them where crashes and interleavings are explored.

## Bug 2: Livelock via Unrepaired Sub-Quorum Entries

### What Happened

After the HEIGHT_EXISTS fix prevented forks, chaos test seed 8 revealed a permanent production stall. A previous leader had written a block at height 24 to some Redis nodes before failing. The entry existed on the nodes but below quorum. The new leader:

1. Read streams during reconciliation — saw the entry on fewer than quorum nodes, ignored it
2. Tried to produce at height 24 — `HEIGHT_EXISTS` rejected the write on nodes that had the orphaned entry
3. Could not reach quorum on the remaining nodes
4. Production permanently stalled

### Why the Model Missed It

**Flaw 3 — No sub-quorum repair**: The `Promote` action (lines 118-166) reads streams and sets `next_height` based on quorum-committed heights only. Sub-quorum entries are silently ignored. The model never considers the scenario where an ignored orphan later blocks production via HEIGHT_EXISTS.

**Flaw 4 — `deadlock_detection: false`**: Line 36 of the original spec explicitly disables deadlock detection. Combined with `MAX_BLOCKS = 2`, the model never flags terminal states where no action is enabled. The livelock — a state where the leader cannot produce and no other action can resolve the situation — is exactly a deadlock in the model's state space. With detection disabled, it's invisible.

**Flaw 5 — No per-sequencer `local_db`**: Without tracking each sequencer's local database, there is no concept of "stuck at a height." The model's `next_height` counter only advances on quorum success, but there's no mechanism to express that a particular height is permanently blocked by orphaned entries.

### The Fix

1. **Sub-quorum repair during reconciliation**: When `unreconciled_blocks` finds a height with entries below quorum, the new leader repropose the highest-epoch entry to all nodes. Nodes that already have an entry return HEIGHT_EXISTS (counted as "has it"), nodes missing it accept the write. Once quorum is reached, the block is available for reconciliation.

2. **Deadlock detection enabled**: The new spec does not disable deadlock detection. Terminal states where no action is enabled are flagged as failures. If the repair logic is removed, the model checker produces a counterexample showing the livelock.

## Taxonomy of Abstraction Flaws

| # | Flaw | Original Model | Real System | Impact |
|---|------|---------------|-------------|--------|
| 1 | **Atomic commit fallacy** | `ProduceBlock` is one atomic step | Redis publish and local commit are separate operations with a crash window between them | Hid the interleaving where a new leader promotes between quorum write and local commit |
| 2 | **No per-sequencer local DB** | Only `next_height` counter | Each sequencer has a persistent RocksDB with committed blocks | Fork assertion checked Redis (coordination state) instead of local DBs (client-observable state) |
| 3 | **No HEIGHT_EXISTS** | `write_block` always appends | `write_block.lua` must reject writes at occupied heights | Two different blocks at the same height could both achieve quorum |
| 4 | **No sub-quorum repair** | `Promote` ignores sub-quorum entries | New leader must repropose orphaned entries to reach quorum | Orphaned entries permanently block production |
| 5 | **Deadlock detection disabled** | `deadlock_detection: false` | Production stalls are observable failures | Livelock states invisible to the model checker |
| 6 | **NoFork checks Redis, not local state** | Assertion counts quorum entries in Redis streams | Fork = two sequencers with different blocks at the same height in their local DBs | A fork between local databases could exist even if Redis streams look consistent |

## Lessons Learned

1. **Models must match the code's actual commit boundaries.** If the real system has a yield point between two operations, the model must too. Collapsing sequential steps into atomic transitions eliminates exactly the interleavings where bugs hide.

2. **Safety properties must be defined over client-observable state.** The NoFork assertion should check what clients see (sequencer local databases), not internal coordination state (Redis streams). A system can have consistent coordination state and still present inconsistent views to clients.

3. **Never disable deadlock detection without an alternative liveness assertion.** `deadlock_detection: false` silences an entire category of bugs. If the model's bounded exploration makes deadlock detection noisy, add explicit liveness assertions (`eventually` properties) rather than disabling the check entirely.

4. **Formal verification and chaos testing are complementary.** The model found bugs that chaos testing would take longer to hit (specific interleaving sequences). Chaos testing found bugs that the model missed (abstraction errors). Neither is sufficient alone. The ideal workflow: model first, chaos test the implementation, update the model when chaos testing finds gaps.

5. **"The model passed" does not mean "the design is correct."** A model can only verify properties about the system it describes. If the model omits a real-world mechanism (HEIGHT_EXISTS, sub-quorum repair, local persistence), passing tells you nothing about that mechanism's correctness.

## Corrected Model

The corrected specification is in [`sequencer_ha_v2.fizz`](sequencer_ha_v2.fizz). It addresses all six flaws and has been validated via mutation testing — removing HEIGHT_EXISTS produces a NoFork counterexample, and removing the repair logic triggers deadlock detection.
