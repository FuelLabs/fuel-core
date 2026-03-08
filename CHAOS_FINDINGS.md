# Chaos Test Findings — HA Leader Lock Failover

**Date**: 2026-03-07
**Tool**: `cargo run -p fuel-core-chaos-test`
**Branch**: `poa-quorum-fixes`

---

## Summary

12 chaos test sessions were run with varying parameters. After fixing a framework
issue (in-memory DBs causing false fork detections), persistent RocksDB-backed nodes
revealed **one real system-level finding**: nodes that fall behind during fault
injection fail to recover and sync, staying permanently stuck at stale heights.

**Fork safety is confirmed**: zero fork violations were observed across all sessions
using persistent storage.

---

## Finding 1: Permanent Node Sync Stall After Fault Disruption

**Severity**: High
**Reproducible**: Yes (multiple seeds)
**Category**: Liveness / Recovery

### Description

Under fault injection (network partitions, Redis kills, latency injection, node
restarts), one or more nodes stop advancing their block height and **never recover**,
even after all faults are cleared during the 10-second settling period. The stuck
nodes remain alive (process running) but do not import new blocks from peers or
produce blocks locally.

### Pattern

The stuck node typically:
1. Reaches a low block height early in the test (height 5–55)
2. Encounters a combination of faults (partition + Redis failure + possible kill/restart)
3. Never advances again, even as other nodes continue producing (reaching heights 200–560)
4. Remains stuck through the settling period when all proxies are restored to Normal

### Reproduction

```bash
# Aggressive — fails 3/4 sessions
cargo run -p fuel-core-chaos-test -- --seed 31415 --duration 90s \
  --fault-interval 500ms --block-time 100ms

# Moderate — fails with seed 99999 even at 2s fault interval
cargo run -p fuel-core-chaos-test -- --seed 99999 --duration 90s \
  --fault-interval 2s --block-time 200ms
```

### Affected Seeds & Results

| Seed  | Fault Interval | Block Time | Stuck Nodes           | Max Global Height | Result |
|-------|---------------|------------|----------------------|-------------------|--------|
| 31415 | 500ms         | 100ms      | Node 0 @ h=7, Node 2 @ h=11 | 560         | FAIL   |
| 12345 | 500ms         | 100ms      | Node 0 @ h=12               | 177         | FAIL   |
| 99999 | 500ms         | 100ms      | Node 1 @ h=21, Node 2 @ h=110 | 415       | FAIL   |
| 99999 | 2s            | 200ms      | Node 1 @ h=55               | 216         | FAIL   |

### Passing Sessions (no violations)

| Seed  | Fault Interval | Block Time | Faults | Blocks | Result |
|-------|---------------|------------|--------|--------|--------|
| 1     | 2s            | 200ms      | 14     | 436    | PASS   |
| 42    | 2s            | 200ms      | 16     | 399    | PASS   |
| 777   | 2s            | 200ms      | 21     | 465    | PASS   |
| 1337  | 1s            | 200ms      | 50     | 394    | PASS   |
| 9999  | 1s            | 200ms      | 40     | 290    | PASS   |
| 65536 | 500ms         | 100ms      | 105    | 491    | PASS   |
| 54321 | 500ms         | 100ms      | 126    | 204    | PASS   |
| 31415 | 2s            | 200ms      | 36     | 548    | PASS   |

### Hypothesis

The sync stall likely involves one or more of:

1. **P2P sync not triggered after partition heals**: After a network partition is
   lifted (proxy restored to Normal), the node's P2P layer may not re-initiate sync
   with peers that have advanced significantly ahead.

2. **Leader lock state prevents block production**: The node may have lost its lease
   and be stuck in a retry loop that doesn't converge after the Redis connectivity
   is restored, preventing it from either producing blocks or recognizing it should
   sync from peers.

3. **Connection caching in Redis client**: The Redis multiplexed connection driver
   may cache a broken connection after proxy-induced failures and not reconnect
   after the proxy returns to Normal mode. (Observed log: "Multiplexed connection
   driver unexpectedly terminated- IoError" repeating indefinitely.)

4. **Backpressure / task starvation**: Under high fault rates, the tokio runtime
   may have a buildup of pending reconnection tasks or timed-out futures that prevent
   the sync task from making progress.

### Impact

In a production HA deployment, if a node gets partitioned from Redis and then
reconnects, it may permanently stop participating in block production and syncing.
This reduces the effective cluster size and could lead to a single point of failure
if the remaining nodes also experience faults.

---

## Fork Safety: CONFIRMED

Zero fork violations were observed across all 12 sessions with persistent storage.
The leader lock correctly prevents two nodes from producing different blocks at
the same height, even under aggressive fault injection including:
- Simultaneous Redis kills (up to quorum boundary)
- Asymmetric network partitions
- Mid-operation TCP connection drops
- Added latency (50–500ms) on Redis connections
- Node kill/restart cycles

---

## Framework Notes

### False Positives Fixed

Early sessions using in-memory databases produced 152 false fork violations
(seed=65536). These were caused by restarted nodes replaying from genesis with
different block IDs because in-memory state was lost. Switching to persistent
RocksDB via `TempDir` + `CombinedDatabase::open()` eliminated all false positives.

### Chaos Test Configuration

- 3 PoA nodes, 3 Redis instances, 9 TCP proxies (per-node-per-redis)
- Lease TTL: 2s, node timeout: 50ms, retry delay: 100ms
- Gap tolerance: 60s (violations flagged when a live node is >10 blocks behind
  for >60s)
