# Chaos Test Findings — HA Leader Lock Failover

**Date**: 2026-03-08
**Tool**: `cargo run -p fuel-core-chaos-test`
**Branch**: `poa-quorum-fixes`

---

## Summary

20 chaos test sessions were run across multiple rounds of framework fixes.
After eliminating two classes of false positives (in-memory DB restarts and
lossy broadcast stream), the chaos test passes clean across all seeds and
fault intensities.

**No real invariant violations found.** Fork safety and liveness both hold
under aggressive fault injection.

---

## Fork Safety: CONFIRMED

Zero fork violations observed across all 20 sessions with persistent storage.
The leader lock correctly prevents two nodes from producing different blocks at
the same height, even under aggressive fault injection including:
- Simultaneous Redis kills (up to quorum boundary)
- Asymmetric network partitions (node A can reach Redis 1 but node B cannot)
- Mid-operation TCP connection drops (CloseAfterBytes)
- Added latency (50–500ms) on Redis connections
- Node kill/restart cycles with persistent RocksDB state

---

## Liveness: CONFIRMED

All nodes successfully sync and recover after faults are cleared. Gap detection
uses direct RocksDB height polling (not the broadcast stream) to verify nodes
advance. No real sync stalls detected across 8 aggressive sessions (500ms fault
interval, 100ms block time).

---

## Framework Bugs Fixed (False Positives)

### 1. In-Memory DB Restart Fork False Positives

**Round 1 finding**: 152 fork violations (seed=65536) and height regressions.

**Root cause**: Nodes used in-memory databases. On restart, all state was lost
and the node replayed from genesis, producing blocks with different IDs at
already-seen heights.

**Fix**: Switched to persistent RocksDB via `TempDir` + `CombinedDatabase::open()`.
The same temp directory is reused across restarts so chain state survives.

### 2. Broadcast Stream Lag False Positives (Gap Violations)

**Round 2 finding**: Gap violations on seeds 31415, 12345, 99999 — nodes
appeared stuck at low heights (5–55) while other nodes advanced to 200+.

**Root cause**: The invariant checker subscribed to block events via
`block_importer.block_stream()`, which is a `BroadcastStream` wrapping a
tokio broadcast channel. When the consumer falls behind, `BroadcastStream`
returns `Err(Lagged(n))`, and the `.filter_map(|r| r.ok())` in the stream
adapter silently drops these errors. The invariant checker missed blocks and
reported stale heights.

**Proof**: Debug logs showed Node-1's `ImportTask` successfully committed blocks
up to height 253 (`Committed(253)`), but the invariant checker's stream
subscriber only saw up to height 55.

**Fix**: Replaced stream-based gap detection with direct RocksDB polling via
`database.on_chain().latest_height()` (the `HistoricalView` trait). This reads
the committed height directly from the database, bypassing the lossy broadcast
channel entirely. Stream-based subscriptions are still used for fork detection
and concurrent-leader checks (where individual block identity matters), but
gap/liveness checking uses the authoritative DB state.

---

## All Session Results (Final Round — DB-Polling Gap Detection)

| Seed  | Fault Interval | Block Time | Faults | Blocks | Result |
|-------|---------------|------------|--------|--------|--------|
| 12345 | 500ms         | 100ms      | 71     | 263    | PASS   |
| 54321 | 500ms         | 100ms      | 87     | 280    | PASS   |
| 65536 | 500ms         | 100ms      | 68     | 331    | PASS   |
| 99999 | 500ms         | 100ms      | 82     | 224    | PASS   |
| 31415 | 500ms         | 100ms      | 85     | 187    | PASS   |
| 1337  | 500ms         | 100ms      | 85     | 217    | PASS   |
| 9999  | 500ms         | 100ms      | 70     | 205    | PASS   |
| 777   | 500ms         | 100ms      | 79     | 299    | PASS   |

Earlier passing sessions (2s fault interval):
1, 42, 777, 1337, 9999, 31415, 54321, 65536, 99999 — all PASS.

---

## Chaos Test Configuration

- 3 PoA nodes, 3 Redis instances, 9 TCP proxies (per-node-per-redis)
- Persistent RocksDB via TempDir (survives node restarts)
- Lease TTL: 2s, node timeout: 50ms, retry delay: 100ms
- Gap tolerance: 60s (via direct DB height polling)
- P2P sync via bootstrap relay + gossipsub between nodes
- Run: `cargo run -p fuel-core-chaos-test -- --seed 42 --duration 60s`
