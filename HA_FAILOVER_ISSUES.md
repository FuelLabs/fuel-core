# HA Failover Issues — Redis Fencing Token Approach

Observed during devnet testing of PR [#3210](https://github.com/FuelLabs/fuel-core/pull/3210) (implementation) and PR [#3209](https://github.com/FuelLabs/fuel-core/pull/3209) (spec).

After toggling failover between nodes several times, two issues were observed:

---

## Issue 1: Inconsistent Failover Time (5s vs 20s)

### Observation

Sometimes failover took ~5s, other times ~20s for the new leader to become responsive.

### Root Cause Analysis

The failover latency is: `lease_ttl + (0..trigger_interval) + acquisition_time`.

Several factors contribute to the variability:

#### 1. Follower only checks once per trigger interval

When `leader_state` returns `ReconciledFollower`, the PoA service returns `TaskNextAction::Continue` and the main loop waits for the **full next trigger deadline** before retrying (`service.rs` main `run` method):

```rust
Trigger::Interval { block_time } => {
    let next_block_time = self.last_block_created.checked_add(block_time)...;
    Box::pin(async move { sleep_until(next_block_time).await; Instant::now() })
}
```

There is no fast-retry path for followers — they must wait the entire interval before checking again.

#### 2. The importer's `active_import_results` semaphore has a 20s timeout

In `importer.rs`, both `commit_result` and `execute_and_commit` wait up to 20 seconds for the backpressure semaphore:

```rust
const TIMEOUT: u64 = 20;
let await_result = tokio::time::timeout(
    Duration::from_secs(TIMEOUT),
    self.active_import_results.clone().acquire_owned(),
).await;
```

If downstream subscribers (GraphQL worker, off-chain indexer, etc.) are slow processing previous block notifications, new block commits block here for up to 20s. This can affect both the old leader (which stays "alive" but stuck, continuing to renew its lease via `write_block.lua`'s `PEXPIRE`) and the new leader during reconciliation (via `execute_and_commit`).

#### 3. `production_timeout` defaults to 20s

If the old leader is mid-block-production when killed, `signal_produce_block` has a 20-second timeout wrapper. If the new leader was previously a node that restarted, any in-flight production blocks the loop.

#### 4. 1-second unconditional sleep on error

In `handle_normal_block_production` (`service.rs`):

```rust
Err(err) => {
    tokio::time::sleep(Duration::from_secs(1)).await;
    TaskNextAction::ErrorContinue(err)
}
```

### Potential Fixes

1. **Add a faster retry loop for followers**: When `ReconciledFollower` is returned, retry acquisition with a short backoff (e.g., 500ms) up to a limit, rather than waiting the full trigger interval.
2. **Reduce or decouple the importer semaphore timeout**: Either lower the 20s constant or decouple block notification processing from the commit path so slow subscribers can't block production.

---

## Issue 2: Fork Despite Fencing Tokens

### Observation

After repeated failover toggling, a chain fork occurred — two nodes diverged on the block at the same height.

### Version Context

The reconciliation logic differs between two versions on this branch:

- **Merge commit `d78460b2`** ("Add fork resilience for Leader Lock #3210") had fork-resilient reconciliation using `HashMap<u32, HashMap<u64, SealedBlock>>` and `(epoch, block_id)` quorum voting.
- **Post-merge commit `47c26b3844`** ("make stream lookup more efficient") **regressed** this to `HashMap<u32, (u64, SealedBlock)>` with a simple `max_by_key(epoch)` — reintroducing the fork vulnerability.

The current branch tip (`feature/unbroadcasted-block`) has the regressed code. **If devnet was running a build from or after `47c26b3844`, the fork scenario below applies.** If devnet was running the exact merge commit `d78460b2`, a different fork path (e.g., stale-node/trimmed-stream gap) would need to be investigated.

### Root Cause (current branch tip, post-`47c26b3844`)

The epoch token only increments on **leadership acquisition** (`promote_leader.lua` does `INCR`), not per block. All blocks produced by the same leader during the same term share the same epoch value. This means the fencing token cannot distinguish two different blocks at the same height produced during the same leadership term.

Combined with the fact that `write_block.lua` does not check for duplicate heights in the stream (it always does `XADD`), a **partial publish failure** can leave a stale block in a Redis node's stream. A subsequent retry by the same leader produces a different block at the same height with the same epoch, creating an ambiguous state that reconciliation resolves incorrectly.

### Fork Scenario (Step by Step)

1. **Leader A** acquires lease on 3 Redis nodes, epoch=5.
2. A produces block at height N (call it `block_N_a`).
3. A calls `_commit_result` → `publish_produced_block`:
   - Node 1: `write_block.lua` → identity check passes, epoch check (5≥5) passes → **XADD succeeds**, `block_N_a` written to stream.
   - Node 2: **connection timeout** → fails.
   - Node 3: **connection timeout** → fails.
   - successes=1 < quorum(2) → **publish returns Err**.
4. `_commit_result` returns error → **`block_N_a` is NOT committed to A's local DB**.
5. **But `block_N_a` remains in Node 1's Redis stream** (no rollback mechanism).
6. Next trigger fires. A calls `leader_state(N-1, N)`:
   - `can_produce_block` → `renew_lease_if_owner` → quorum of nodes renew → A is still leader.
   - `unreconciled_blocks(N)`: Node 1 has `block_N_a`, nodes 2 and 3 don't → only 1/3 → **quorum not reached** → returns empty.
   - Returns `ReconciledLeader`.
7. A produces a **different** block at height N (call it `block_N_b` — different timestamp, possibly different transactions from the txpool).
8. A publishes `block_N_b`:
   - Node 1: `write_block.lua` → epoch check (5≥5) passes → **XADD appends** `block_N_b`. Node 1 now has **both** `block_N_a` and `block_N_b` at height N, both with epoch 5.
   - Node 2: succeeds.
   - Node 3: succeeds.
   - quorum reached → **publish succeeds**.
9. A commits `block_N_b` to local DB.

**Later, when Leader B becomes leader and reconciles:**

The per-node fold in `unreconciled_blocks` keeps the **first** entry at each height on epoch ties:

```rust
// Current branch (47c26b3844):
Some((current_epoch, _)) if *current_epoch >= epoch => {} // skip — keeps first entry
```

- **Node 1's fold**: sees `block_N_a` (epoch 5) first, then `block_N_b` (epoch 5) — keeps `block_N_a`.
- **Node 2's fold**: only has `block_N_b` (epoch 5) — keeps `block_N_b`.
- **Node 3's fold**: only has `block_N_b` (epoch 5) — keeps `block_N_b`.

Then `max_by_key` selects the candidate with the highest epoch. All have epoch 5 (tie), so `max_by_key` returns the **last** element per Rust's iterator semantics. Depending on node ordering in `self.redis_nodes`, B could reconcile either `block_N_a` or `block_N_b`.

**If B reconciles `block_N_a`**: B has `block_N_a` at height N, A has `block_N_b` at height N → **FORK**.

### Regression: What `47c26b3844` Removed

The merge commit `d78460b2` had three protections that `47c26b3844` stripped out:

1. **Nested map** `HashMap<u32, HashMap<u64, SealedBlock>>`: same-epoch entries at the same height overwrote each other (last wins), so the latest block_b would replace block_a.
2. **Block-ID quorum voting**: reconciliation counted votes by exact `(epoch, block_id)` and required quorum agreement on the same block content, not just the same height.
3. **`BlockId` import**: removed entirely.

### Unit Test

A passing unit test proving the fork on the current branch code exists at:

```
poa::tests::partial_publish_then_retry_at_same_height__new_leader_reconciles_stale_block
```

Run with: `cargo test --features leader_lock -p fuel-core partial_publish_then_retry_at_same_height`

### Why the Fencing Token Doesn't Prevent This

The fencing check in `write_block.lua` only rejects writes with a **lower** epoch than the current token. It does **not**:

- Check for duplicate heights in the stream (multiple `XADD` entries at the same height are allowed).
- Prevent the same leader from writing multiple different blocks at the same height within the same epoch.
- Increment the epoch per block (only per leadership acquisition).

### Potential Fixes

**Option A — Revert `47c26b3844`'s reconciliation changes:** Restore the `HashMap<u32, HashMap<u64, SealedBlock>>` nested map and `(epoch, block_id)` quorum voting from the merge commit `d78460b2`. This was already working correctly before the "efficiency" refactor.

**Option B — Release lease on publish failure:** If `publish_produced_block` fails to reach quorum, immediately release the lease and invalidate the cached epoch token. This forces re-acquisition (with a new, higher epoch) before the leader can retry at the same height.

**Option C — Per-block epoch increment:** Increment the epoch atomically in `write_block.lua` on each block write rather than only on leadership acquisition. Each block gets a globally unique epoch, making reconciliation deterministic.

**Option D — Height-uniqueness check in Lua:** Before the `XADD` in `write_block.lua`, scan the stream for an existing entry at the same height and same epoch. If one exists, reject the write.

---

## Issue 2b: Fork on `release/v0.47.2` (Devnet) — Different Root Cause

### Context

Devnet was running `release/v0.47.2`, which includes merge commit `d78460b2` with the robust `(epoch, block_id)` quorum voting reconciliation — NOT the regressed code from `47c26b3844`. The fork scenario described in Issue 2 above (same-epoch partial publish + retry) is handled correctly by the release branch's `HashMap<u32, HashMap<u64, SealedBlock>>` fold + block-ID voting.

### Analysis: Fencing Is Sound for Standard Dual-Write Scenarios

The identity check in `write_block.lua` is atomic (Lua script = single Redis op) and prevents two leaders from both reaching publish quorum. With `SET NX` acquisition and majority quorum (`N/2 + 1`), two leaders cannot simultaneously hold the lease on overlapping node sets. Every interleaving traced (partial lease expiry, sequential publish, concurrent acquisition) results in at most one leader reaching quorum on `publish_produced_block`.

### Remaining Fork Candidates

#### Candidate A: Redis Node Data Loss (Restart Without Persistence)

If any Redis node restarted without AOF/RDB persistence during testing, it loses its stream data. A block published to exactly quorum nodes (e.g., 2/3) would then exist on only 1 node — below quorum. The new leader's reconciliation can't find it, becomes `ReconciledLeader`, and produces a divergent block at the same height.

**Example:**
1. A publishes block N to nodes 1,2 (quorum=2). Commits locally.
2. Redis node 2 restarts (data lost).
3. Block N now only on node 1 (1 < quorum).
4. B acquires lease, reconciles: `nodes_with_height = 1` → break → `ReconciledLeader`.
5. B produces different block N → **fork**.

**Diagnostic:** Check if Redis nodes have persistence (`appendonly yes` or `save` directives). Check Redis uptime during the test window.

#### Candidate B: `can_produce_block` TOCTOU (No TTL Renewal)

On `release/v0.47.2`, `can_produce_block` uses `has_lease_owner_quorum()` which only does `GET` — it does **not** renew the lease TTL. The TTL is only renewed inside `write_block.lua`'s `PEXPIRE` (at the very end of a successful publish). There is no `renew_lease.lua` on this branch.

This means the lease decays between the ownership check and the actual publish. If block production takes longer than the remaining TTL, the lease expires and another node can acquire. The identity check in `write_block.lua` catches this (the stale leader's write is rejected), so it doesn't directly cause a fork. However, it contributes to:
- Unpredictable failover timing (Issue 1)
- Unnecessary publish failures that leave orphaned partial writes in Redis streams

**Fix:** Add TTL renewal in `can_produce_block` (as done on the `feature/unbroadcasted-block` branch with `renew_lease_if_owner`).

#### Candidate C: `XTRIM MAXLEN ~` Approximate Trimming

The `~` flag in `XTRIM` means Redis trims approximately — different nodes could retain different entries near the trim boundary. If a block at a specific height is trimmed from some nodes but not others, reconciliation sees inconsistent data. Combined with the `nodes_with_height` pre-check, a block that should have quorum might appear below quorum due to uneven trimming.

**Diagnostic:** Check `stream_max_len` configuration. If blocks are near the trim boundary, this is a risk.

### Root Cause (Most Likely): Silent Read Failures in Reconciliation

`read_stream_entries_on_node` silently returns an empty `Vec` on connection timeout or error:

```rust
// poa.rs — read_stream_entries_on_node
Ok(Err(_)) | Err(_) => {
    self.clear_cached_connection(redis_node).await;
    return Vec::new(); // ← treated as "this node has no blocks"
}
```

This empty Vec feeds into `unreconciled_blocks` where `nodes_with_height` counts it as a node with **no blocks at any height**. A transient read failure on 2 of 3 nodes makes every height appear below quorum, causing the new leader to skip reconciliation and produce divergent blocks.

#### Fork Scenario

3 Redis nodes, quorum=2. Leader A committed block N (published to all 3 nodes).

1. B acquires lease, calls `unreconciled_blocks(N)`.
2. `read_stream_entries_on_node`:
   - Node 1: succeeds → has block N.
   - Node 2: **connection timeout** → returns `Vec::new()`.
   - Node 3: **connection timeout** → returns `Vec::new()`.
3. `nodes_with_height` for height N = 1 (only node 1 reported data).
4. `1 < quorum(2)` → break → returns empty.
5. Returns `ReconciledLeader`.
6. B produces a **different** block at height N → **FORK**.

P2P sync could have caught this, but the PoA task doesn't wait for sync to complete before producing.

#### Why This Is Especially Dangerous

- The write path (fencing) is sound — `write_block.lua` correctly prevents dual-quorum writes.
- The **read path** silently degrades. A node that can write to Redis quorum (proving connectivity) may have just failed to read from quorum moments earlier.
- Network instability during failover (the exact moment reads happen) is the most likely trigger.

### Fix

`unreconciled_blocks` must track how many Redis nodes were **successfully read** and require a quorum of successful reads before concluding "no blocks exist." If reads fail on too many nodes, return an error (causing retry/follower state) rather than proceeding to produce.

```rust
// Pseudocode fix:
let (blocks_by_node, read_failures): (Vec<_>, Vec<_>) = results.partition(...);
if !self.quorum_reached(blocks_by_node.len()) {
    return Err(anyhow!("Cannot reconcile: failed to read quorum of Redis nodes"));
}
```

### Additional Hardening

1. **Wait for P2P sync before producing:** After acquiring the lease, wait for at least one P2P sync round to complete. This provides a second chance to receive blocks that Redis reconciliation might miss.
2. **Add TTL renewal in `can_produce_block`:** Replace `has_lease_owner_quorum()` with a TTL-renewing check to prevent lease decay between ownership verification and block publish.
3. **Require quorum reads, not just quorum heights:** The `nodes_with_height` check is necessary but not sufficient. The pre-check should also verify that quorum nodes were successfully contacted.

---

## Issue 3: Node Permanently Bricked After Graceful Shutdown — `NoHistoryForRequestedHeight` Panic

### Observation

Chaos testing (seed 3) shows that after a graceful shutdown (`send_stop_signal_and_await_shutdown`), restarting a node on the same data directory causes a panic in `GraphQL_Off_Chain_Worker` initialization, which cascades to shut down the entire `FuelService`. The node is **permanently bricked** — every subsequent restart crashes identically within ~15ms, before any Redis or P2P operations execute.

### Reproduction

```bash
# Build the chaos test binary
cargo build -p fuel-core-chaos-test

# Seed 3 reliably reproduces this (nodes 0 and 2 get bricked)
cargo run -p fuel-core-chaos-test -- --seed 3 --stall-threshold 6s

# Seed 200 also hits this
cargo run -p fuel-core-chaos-test -- --seed 200 --stall-threshold 6s
```

### Root Cause

When a node is gracefully stopped, a block commit can land during the shutdown sequence. This leaves the on-chain database at height N, but the off-chain historical state database lacks the entry for height N-1 (the previous block the off-chain worker needs for initialization).

On restart, `GraphQL_Off_Chain_Worker` tries to read history at `committed_height - 1`, gets `NoHistoryForRequestedHeight`, and panics. The panic cascades via `FuelService` to shut down all sub-services (PoA, SyncTask, ImportTask, P2P).

### Evidence from Seed 3

| Time | Event |
|------|-------|
| t=13.6s | Node 0 stopped at height 38. Block 39 commits during shutdown. |
| t=27.6s | Node 2 stopped at height 79. Block 80 commits during shutdown. |
| t=29.4s | Node 0 restarted → panics: `NoHistoryForRequestedHeight { requested_height: 38 }` → full shutdown |
| t=35.5s | Node 2 restarted → panics: `NoHistoryForRequestedHeight { requested_height: 79 }` → full shutdown |
| t=29-310s | Node 0 restarted **10 times**, Node 2 restarted **6 times** — every attempt crashes instantly |

Node 1 survives because it was always shut down cleanly with history intact. It becomes the sole producer (reaching height 428) while Nodes 0 and 2 are permanently dead.

### Key Detail: This Is a Graceful Shutdown

The chaos framework does **not** forcibly kill nodes. `FaultAction::KillNode` calls `cluster.stop_node()`, which calls `send_stop_signal_and_await_shutdown()`. The bug occurs even with cooperative shutdown — the issue is that the shutdown sequence allows a block commit to complete while the off-chain worker is shutting down (or has already shut down), leaving the databases in an inconsistent state.

### Impact

- **Permanent node failure**: Once bricked, the node cannot recover without manual data directory cleanup.
- **Cascading production stalls**: If the sole remaining producer is also killed, there are zero functional nodes. The chaos framework keeps restarting bricked nodes, but they crash instantly every time. In seed 3, this caused a 58s stall (height 110) and a 117s stall (height 188).
- **Redis quorum and P2P are irrelevant**: The crash happens during service initialization, before any network operations.

### Potential Fixes

1. **Don't panic on missing history**: `GraphQL_Off_Chain_Worker` initialization should handle `NoHistoryForRequestedHeight` gracefully — skip to the next available height or start from the current height.
2. **Don't cascade non-critical panics**: A failure in the off-chain indexer should not shut down core consensus, sync, and P2P services. The node should continue operating even if the GraphQL worker can't initialize.
3. **Atomic shutdown gating**: Prevent new block commits from landing once the stop signal has been sent. If the off-chain worker has already shut down, the commit should not proceed.

### Status

**Partially mitigated** — the `NoHistoryForRequestedHeight` panic was fixed by enabling `StateRewindPolicy::RewindFullRange` in the chaos test cluster config (commit `7358294006`). Nodes no longer brick on restart. However, the underlying issue of blocks committing during shutdown remains open and may contribute to other edge cases.

---

## Issue 4: Fork from Silent Reconciliation Read Failure During Failover

### Observation

Chaos testing (seed 8) produces a **chain fork** — two nodes commit different blocks at the same height. This is a critical safety violation. The fork is non-deterministic but reproduces in roughly 2 out of 3 runs.

### Reproduction

```bash
# Seed 8 reproduces this fork ~66% of the time
# Run in a loop to catch it reliably:
for i in $(seq 1 5); do
    cargo run -p fuel-core-chaos-test -- --seed 8 --stall-threshold 6s --duration 2m
    if [ $? -ne 0 ]; then echo "Fork reproduced on attempt $i"; break; fi
done
```

### Root Cause: Under Investigation

The design correctly publishes to Redis quorum BEFORE committing locally — the `_commit_result` function calls `publish_produced_block` first, and only commits to RocksDB if the publish succeeds. Block 322 **was** published to Redis quorum (confirmed by the "Committed block" log, which only fires after successful Redis publish).

The silent read failure path (Issue 2b) has been closed: `read_stream_entries_on_node` now returns `Err` on failure (commit `48209e5f35`), and `unreconciled_blocks` already requires a quorum of successful reads before proceeding (line 586).

Despite both safeguards being in place, the fork still occurs. Possible remaining vectors:

1. **XTRIM race**: Block 322 was published but `XTRIM MAXLEN ~` on some Redis nodes removed it before Node 2's reconciliation read (~1.1s later). The approximate `~` flag means different nodes may trim different entries.
2. **Successful reads that miss the block**: Node 2 gets a quorum of successful reads, but the block exists only on Redis nodes that were not in that quorum (e.g., published to Redis 0 and 1, but Node 2 successfully reads from Redis 1 and 2 where Redis 2 doesn't have it yet or was trimmed).
3. **Publish quorum vs read quorum asymmetry**: The block was published to exactly quorum nodes (2/3), and the reading node happened to read a different quorum subset that doesn't overlap.

### Fork Sequence (Seed 8, Height 322)

| Time | Event |
|------|-------|
| t+81.5s | Node 0 is leader, commits block 321. |
| t+81.7s | **Kill Node 0 injected** — graceful shutdown begins |
| t+81.9s | Block 322 (`68a3cded...`) committed — Redis publish **succeeded**, then local DB commit |
| t+81.9s | PoA shuts down. Block 322 exists in both Redis and Node 0's local DB. |
| t+82.1s | Node 1 logs: "Timed out while connecting to redis leader-lock node" — Redis connectivity degraded |
| t+82.5s | Node 2 restarts, P2P-syncs blocks 302-321 from Node 1 |
| t+83.0s | Node 2 acquires Redis lease, calls `unreconciled_blocks(322)` |
| t+83.0s | `read_stream_entries_on_node` fails silently on enough nodes → returns empty → `ReconciledLeader` |
| t+83.0s | **Node 2 produces a different block 322** (`c79b4850...`) → **FORK** |

### Key Evidence

1. The "Committed block" log at `19:45:06.975` proves Redis publish succeeded — `_commit_result` publishes first, then commits.
2. Node 1 had "Timed out while connecting to redis leader-lock node" at `19:45:07.200` — confirming Redis connectivity issues in this window.
3. No `read_stream_entries` error logs appear for Node 2 — the failures are **silent** (errors return empty Vec, not logged at info level).

### Cascading Consequences

1. **Fork at height 323**: Node 0 restarts with divergent block 322, produces block 323 on top of it (committed during a subsequent shutdown). Nodes 1 and 2 have a different block 323.

2. **Permanent divergence**: Node 0 can never sync from peers — every import attempt fails with "Previous root of the next block should match the previous block root."

3. **Production stall**: When the remaining leader loses Redis quorum, no node can produce, causing a 25s stall at height 367+.

### Existing Safeguards (Already In Place)

Both the read-side and reconciliation-side fixes are already implemented:

1. `read_stream_entries_on_node` returns `Err` on failure and clears cached connection (commit `48209e5f35`)
2. `unreconciled_blocks` requires a quorum of successful reads before proceeding (line 586 of `poa.rs`)

**The fork occurs despite these safeguards**, which means the root cause is more subtle — likely a quorum overlap issue or stream trimming race rather than a silent failure.

### Next Steps

1. Add tracing to `publish_produced_block` to log which specific Redis nodes received the block
2. Add tracing to `unreconciled_blocks` to log which Redis nodes were successfully read and what heights/blocks they returned
3. Re-run seed 8 with enhanced tracing to identify the exact asymmetry
4. Consider requiring that publish reaches ALL Redis nodes (not just quorum) or that reconciliation reads ALL nodes before concluding no blocks exist at a height
