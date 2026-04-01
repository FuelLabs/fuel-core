# Chaos Test — HA Leader Lock Failover

Standalone binary that continuously injects random faults into a local
PoA cluster with Redis-based leader election, checking safety and liveness
invariants. Think of it as fuzzing for distributed systems.

## Prerequisites

- `redis-server` must be on `$PATH` (`brew install redis`)
- Builds with the `rocksdb` feature (included automatically)

## Quick Start

```bash
# Build
cargo build -p fuel-core-chaos-test

# Run with a specific seed (reproducible)
cargo run -p fuel-core-chaos-test -- --seed 42 --duration 60s

# Run with aggressive fault injection
cargo run -p fuel-core-chaos-test -- --seed 1337 --duration 90s \
  --fault-interval 500ms --block-time 100ms

# Random seed, default settings
cargo run -p fuel-core-chaos-test
```

Exit code 0 = pass, 1 = invariant violations found. The seed is printed
at startup for reproduction.

## What It Does

1. Starts N PoA nodes (default 3), M Redis instances (default 3), and a
   P2P bootstrap relay
2. Creates a per-(node, redis) TCP proxy grid (N x M = 9 proxies) so
   faults can be injected asymmetrically
3. Nodes use persistent RocksDB via temp directories so restarts preserve
   chain state
4. A fault scheduler randomly injects: network partitions, latency,
   mid-operation TCP drops, Redis kills, node kills — then auto-recovers
5. An invariant checker continuously monitors for violations

## Invariants Checked

| Invariant | Method | Severity |
|-----------|--------|----------|
| **No forks** | Block stream: compare block IDs at each height across nodes | Critical |
| **No concurrent leaders** | Block stream: detect multiple nodes locally producing at same height | Critical |
| **No gaps** | DB polling: `on_chain().latest_height()` per node vs global max | Soft (60s tolerance) |
| **No production stalls** | DB polling: global max height must advance within `--stall-threshold` | Soft (default 6s) |

Gap and stall detection use direct RocksDB reads, not the block broadcast
stream (which silently drops events when the consumer lags).

## CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--seed` | random | RNG seed for reproducibility |
| `--duration` | `5m` | Total test duration |
| `--nodes` | `3` | Number of PoA producer nodes |
| `--redis-nodes` | `3` | Number of Redis instances |
| `--block-time` | `200ms` | Block production interval (`Trigger::Interval`) |
| `--fault-interval` | `2s` | Average time between fault injections (±50% jitter) |
| `--stall-threshold` | `6s` | Max allowed time with no blocks from any node |
| `--log-level` | `info` | Tracing filter (`error`, `warn`, `info`, `debug`) |

## Fault Types and Weights

| Category | Weight | Actions |
|----------|--------|---------|
| Network partition | 25% | `PartitionNodeFromRedis`, `PartitionAllFromRedis` |
| Latency injection | 20% | `AddLatency` (50-500ms per direction) |
| Mid-operation drop | 15% | `CloseAfterBytes` (kill after 10-500 bytes) |
| Redis kill/restart | 15% | `KillRedis` / `RestartRedis` |
| Node kill/restart | 15% | `KillNode` / `RestartNode` |
| Restore proxy | 5% | `RestoreProxy` (single) |
| Restore all | 5% | `RestoreAllProxies` |

Safety constraints: never kill below Redis quorum, never kill all nodes,
auto-schedule recovery 5-15s after destructive faults, revert any fault
that breaks Redis quorum for all nodes.

## Architecture

```
bin/chaos-test/src/
  main.rs          Orchestration: startup, fault/invariant spawn, settling, report
  cli.rs           clap CLI definition
  cluster.rs       Cluster lifecycle: Redis + proxy grid + PoA nodes (persistent RocksDB)
  proxy.rs         TCP proxy with switchable fault modes (Normal/DropAll/Latency/CloseAfterBytes)
  fault.rs         Weighted RNG fault scheduler with safety guards and auto-recovery
  invariants.rs    Fork detection (stream), gap/stall detection (DB polling)
  timeline.rs      Event log, violation types, final report
  redis_server.rs  Redis process management (spawn/stop/restart)
```

## Mutation Testing

The harness can validate that known bugs are detectable:

```bash
# Example: reintroduce the silent Redis read failure bug,
# then run the fuzzer to confirm it catches the resulting forks
cargo run -p fuel-core-chaos-test -- --seed 42 --duration 60s \
  --fault-interval 500ms --block-time 100ms
# Expected: FAIL with fork violations
```

## Known Reproducing Seeds

| Seed | What it catches | Fixed? |
|------|----------------|--------|
| 42 | Fork from silent Redis read failure (mutation test) | Yes (quorum read check) |
| 1337 | Production stall from 1s error sleep + lease retention | Yes (release on error + block_time delay) |
| 3, 200 | Production stall from post-restart `ensure_synced` delay | Open (P2P sync gate too slow) |

## Not Triggered by `cargo test`

This is a `[[bin]]` target, not a `[[test]]`. Running `cargo test --all-features`
will not execute the chaos test.
