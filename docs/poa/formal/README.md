# Formal Verification of PoA Sequencer HA Protocol

FizzBee model-checking specification for the high-availability sequencer protocol described in [`../failover.md`](../failover.md).

## What This Verifies

The specification models the **core coordination protocol** — Redlock-based leader election with fencing tokens, Redis Streams as a block log, and epoch self-healing — and exhaustively checks all possible interleavings, crash points, and election storms.

### Safety Properties (Invariants)

| # | Property | Description |
|---|----------|-------------|
| 1 | **NoFork** | At most one committed block per height across all Redis nodes. If two sequencers both achieved write quorum at the same height, it must be the same block. |
| 2 | **EpochMonotonicity** | Epoch counters on each Redis node never decrease between consecutive states (transition assertion). |
| 3 | **LockMutualExclusion** | Each Redis node has at most one live lock owner at any time. |

### Modeling Decisions

- **TTL as nondeterministic expiry**: `RedisNode.ExpireLock` fires nondeterministically — no wall-clock needed.
- **Quorum as subset selection**: `oneof` blocks nondeterministically determine which nodes respond, modeling network partitions/timeouts.
- **Lua script atomicity**: Each Lua script maps to an `atomic func` — this correctly models Redis's single-threaded Lua execution guarantee.
- **Crash injection**: An explicit `Crash` action resets ephemeral sequencer state (locks remain until TTL expiry on nodes).
- **Epoch healing**: Modeled directly in `write_block` — if the caller's epoch exceeds the node's, the node is pulled forward.

## Prerequisites

Install FizzBee via Homebrew (macOS):

```bash
brew tap fizzbee-io/fizzbee
brew install fizzbee
```

Or download a prebuilt binary from the [releases page](https://github.com/fizzbee-io/fizzbee/releases).

Verify installation:

```bash
fizz --help
```

## Running the Specification

```bash
cd docs/poa/formal
fizz --exploration_strategy dfs sequencer_ha.fizz
```

DFS exploration is recommended — it keeps memory usage low (queue depth ~70) and completes in ~22 seconds. BFS works too but uses significantly more memory.

### Expected Output

```
Nodes: 58409, queued: 0, elapsed: 21.787454209s
Time taken for model checking: 21.787495541s
Valid Nodes: 58409 Unique states: 23479
IsLive: true
PASSED: Model checker completed successfully
```

## Interpreting Results

### All properties pass

```
PASSED: Model checker completed successfully
```

This means the model checker exhaustively verified all reachable states (58,409 nodes / 23,479 unique states with default bounds).

### Counterexample found

```
Assertion "NoFork" violated!
Trace:
  State 0: ...
  State 1: ...
  ...
```

The trace shows the exact sequence of actions leading to the violation. Each state in the trace shows the full system state (all Redis nodes and sequencers).

## Configuration Knobs

Edit the constants at the top of `sequencer_ha.fizz` to adjust the state space:

| Constant | Default | Description |
|----------|---------|-------------|
| `NUM_REDIS_NODES` | 3 | Number of independent Redis nodes (N) |
| `QUORUM_SIZE` | 2 | Quorum threshold: `ceil(N/2) + 1` |
| `NUM_SEQUENCERS` | 2 | Number of sequencer processes (1 leader + followers) |
| `MAX_BLOCKS` | 2 | Maximum block height to explore |
| `MAX_ELECTIONS` | 2 | Maximum election rounds |

The `action_options` in the YAML front matter bound how many times each action can fire, keeping the state space tractable. Increase these to explore deeper but expect longer run times and larger state spaces.

**State space scaling**: With default bounds, the spec explores ~58K nodes in ~22s using DFS. Increasing `MAX_BLOCKS` or `MAX_ELECTIONS` by 1 roughly doubles the state space.

## Validating the Spec Is Meaningful

To confirm the spec actually catches bugs, intentionally break a property and verify the model checker finds a counterexample:

**Example — remove the identity check from `write_block`:**

In the `write_block` func on `RedisNode`, comment out the lock-owner check:

```python
# if self.lock_owner != node_id or not self.lock_alive:
#     return {"ok": False, "reason": "LOCK_LOST"}
```

Re-run `fizz --exploration_strategy dfs sequencer_ha.fizz` — the `NoFork` assertion should now produce a counterexample trace showing a zombie leader writing a conflicting block.

**Example — remove the fencing check:**

Comment out the epoch comparison:

```python
# if caller_epoch < self.epoch:
#     return {"ok": False, "reason": "STALE_EPOCH"}
```

This removes the defense-in-depth layer. Depending on the interleaving, the identity check alone may still prevent forks (it's the primary mechanism), but removing both checks will definitely produce counterexamples.

## File Structure

```
docs/poa/formal/
├── sequencer_ha.fizz   # FizzBee specification
└── README.md           # This file
```
