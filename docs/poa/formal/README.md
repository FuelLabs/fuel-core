# Formal Verification of PoA Sequencer HA Protocol

FizzBee model-checking specifications for the high-availability sequencer protocol described in [`../failover.md`](../failover.md).

## Specifications

| File | Description |
|------|-------------|
| `sequencer_ha_v2.fizz` | **Current** — Adversarial model with HEIGHT_EXISTS, de-atomized commits, per-sequencer local_db, and sub-quorum repair |
| `sequencer_ha.fizz` | **Historical** — Original model that passed verification but missed two critical bugs (see [POST_MORTEM.md](POST_MORTEM.md)) |
| `POST_MORTEM.md` | Analysis of six abstraction flaws in the original model |

## What v2 Verifies

The v2 specification models the **implementation-aware** protocol — including the actual commit boundaries, local persistence, and repair mechanisms discovered through chaos testing.

### Safety Properties (Invariants)

| # | Property | Description |
|---|----------|-------------|
| 1 | **NoFork** | No two sequencers have different blocks at the same height in their `local_db`. Defined over client-observable state, not Redis streams. |
| 2 | **RedisNoDoubleQuorum** | At most one distinct block per height has quorum presence across Redis nodes. |
| 3 | **EpochMonotonicity** | Epoch counters on each Redis node never decrease between states. |
| 4 | **LockMutualExclusion** | Each Redis node has at most one live lock owner. |

### Key Modeling Decisions (v2)

| Aspect | v1 (flawed) | v2 (corrected) |
|--------|-------------|----------------|
| **Block commit** | Single atomic `ProduceBlock` action | Split into `ProduceBlock` (Redis write) + `CommitLocal` (local DB) with yield between |
| **Fork definition** | Checked Redis streams | Checks per-sequencer `local_db` (client-observable state) |
| **HEIGHT_EXISTS** | Not modeled | `write_block` scans stream for existing height before append |
| **Sub-quorum repair** | Orphaned entries ignored | `Promote` repropose highest-epoch entry to reach quorum |
| **Deadlock detection** | Disabled | Enabled (catches livelock from unrepaired orphans) |
| **Local persistence** | No `local_db` | Each sequencer has `local_db` dict that persists across crashes |

## Prerequisites

Install FizzBee via Homebrew (macOS):

```bash
brew tap fizzbee-io/fizzbee
brew install fizzbee
```

Or download from the [releases page](https://github.com/fizzbee-io/fizzbee/releases).

## Running the Specification

```bash
cd docs/poa/formal
fizz --exploration_strategy dfs sequencer_ha_v2.fizz
```

DFS exploration is recommended for memory efficiency.

### Expected Output

```
PASSED: Model checker completed successfully
```

## Mutation Testing

The model's value is proven by demonstrating it catches known bugs when safety mechanisms are removed.

### Test 1: Remove HEIGHT_EXISTS (expect NoFork violation)

In `sequencer_ha_v2.fizz`, comment out the HEIGHT_EXISTS check in `write_block`:

```python
# 4. HEIGHT_EXISTS — reject if this height is already in the stream
# for entry in self.stream:
#     if entry["height"] == height:
#         return {"ok": False, "reason": "HEIGHT_EXISTS"}
```

Re-run — the `NoFork` assertion should produce a counterexample showing two sequencers committing different blocks at the same height to their `local_db`.

### Test 2: Remove sub-quorum repair (expect deadlock)

In `sequencer_ha_v2.fizz`, remove the repair loop from `Promote` (the `for h in height_node_sets:` block). Re-run — the model checker should detect a deadlock: a state where the leader cannot produce (HEIGHT_EXISTS blocks it) and no other action can resolve the situation.

### Test 3: Remove identity check (sanity check)

Comment out the lock-owner check in `write_block`:

```python
# if self.lock_owner != node_id or not self.lock_alive:
#     return {"ok": False, "reason": "LOCK_LOST"}
```

Re-run — the `NoFork` assertion should produce a counterexample (zombie leader writes a conflicting block).

## Configuration

| Constant | Default | Description |
|----------|---------|-------------|
| `NUM_REDIS_NODES` | 3 | Number of independent Redis nodes |
| `QUORUM_SIZE` | 2 | Quorum threshold: `ceil(N/2) + 1` |
| `NUM_SEQUENCERS` | 2 | Number of sequencer processes |
| `MAX_BLOCKS` | 2 | Maximum block height to explore |
| `MAX_ELECTIONS` | 3 | Maximum election rounds |

## File Structure

```
docs/poa/formal/
├── sequencer_ha_v2.fizz   # Current adversarial FizzBee specification
├── sequencer_ha.fizz      # Historical v1 specification (flawed)
├── POST_MORTEM.md         # Analysis of v1's abstraction errors
└── README.md              # This file
```
