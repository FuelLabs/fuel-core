# Fuel Core Benches
This directory contains a set of benchmarks for Fuel Core. The benchmarks are
used for calculating gas metering costs for the Fuel Core VM.

## Running the benchmarks
To run the benchmarks, you can use `cargo bench -p fuel-core-benches` command available.
Alternatively you can use `cargo criterion -p fuel-core-benches` if you have it installed.
For more information on using criterion see [the guide](https://bheisler.github.io/criterion.rs/book/).

## `pool_executor_bench` — txpool_v2 + parallel executor (self-contained)

`pool_executor_bench` (`src/bin/pool_executor_bench.rs`) drives the real
`txpool_v2` service and the **parallel executor** together, multi-threaded, on a
programmatically-built chain state. Unlike `tps_bench` it needs **no external
snapshot** — funded user coins and deployed contracts are created at genesis in
memory, so it runs on any checkout with a single command. The workload is a
Zipf-ish mix of single-contract calls, multi-contract calls (2-3 contracts, the
point of the `feature/handle-multi-contract-txs` branch) and plain transfers,
so it exercises the contract-batching / lane-scheduling path.

Assembly level: the **full `FuelService`** (real txpool_v2 service + real
parallel executor + block production through the client), assembled via
`test_helpers::builder::TestSetupBuilder`. The only source touched outside
`benches/` is one additive `lane_scheduler` field on `TestSetupBuilder`
(defaulting to `false`).

A/B the lane scheduler (the two runs are one flag apart):

```sh
cargo run --release -p fuel-core-benches --bin pool_executor_bench -- \
    --txs 2000 --contracts 16 --workers 4 --block-gas 30000000 --lane-scheduler off

cargo run --release -p fuel-core-benches --bin pool_executor_bench -- \
    --txs 2000 --contracts 16 --workers 4 --block-gas 30000000 --lane-scheduler on
```

Key flags (`--help` for all): `--workers/--threads N` (executor worker count),
`--block-gas LIMIT` (total block gas limit, also the per-tx max-gas ceiling),
`--lane-scheduler on|off`, `--txs N`, `--blocks N` (default: run until the pool
drains), `--seed`, `--contracts K`, `--contract-tx-share`,
`--multi-contract-share`, `--script-gas`. Output is a config header, per-block
lines (txs / declared gas / produce time) and a summary (TPS, gas/s, blocks to
drain) plus cumulative parallel-executor metrics.

## Profiling a benchmark
Sometimes it is useful to produce a flamegraph from a benchmark to verify
you are measuring the correct things.
To do this you can use the `cargo flamegraph` [command](https://github.com/flamegraph-rs/flamegraph).

### Benchmarking a specific function
First build the benchmark with debug symbols:
`CARGO_PROFILE_RELEASE_DEBUG=true cargo-criterion -p fuel-core-benches --no-run`
Then you need to find the actual test binary that was built:
`ls target/release/deps/vm* -lat | head -1`
Should give you something like:
`target/release/deps/vm-a17190f2ca5e7169`
Then you can run the benchmark with the `flamegraph` command (runs the bench for 10 seconds):
`flamegraph target/release/deps/vm-a17190f2ca5e7169 --bench bench_name --profile-time 10`
For example if you want to profile the `swwq` op you would do (it's regex):
`flamegraph target/release/deps/vm-a17190f2ca5e7169 --bench '^swwq/swwq$' --profile-time 10`
Or for a dependent bench you would do:
`flamegraph target/release/deps/vm-a17190f2ca5e7169 --bench '^srwq/100$' --profile-time 10`

## Using `collect` to generate bench data
Firstly you should run the benchmarks and collect the output:
`cargo criterion -p fuel-core-benches --message-format json --bench vm > bench.json`
You could just pipe the output into the collect binary but benches are slow to run
so I suggest saving the output like above.

Then you can run the `collect` binary to generate the data you want:
`cargo run -p fuel-core-benches --bin collect --release -- --input bench.json`
This will generate a `gas-costs.yaml` file in the current directory like:
```yaml
burn: 35
call:
  base: 311
  dep_per_unit: 14
```
There are multiple output options. See `collect --help` for more information.

### Generating the `fuel-vm` `default-gas-costs.rs`
Do the same as above except run the following `collect` command.
`cargo run -p fuel-core-benches --bin collect --release -- --input bench.json -f rust --output default-gas-costs.rs`
This will generate a `default-gas-costs.rs` file in the current directory like:
```rust
```

This file can then replace the one in `fuel-vm/src/gas/default-gas-costs.rs`.
