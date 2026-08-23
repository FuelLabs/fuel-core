//! # pool_executor_bench
//!
//! A **self-contained** benchmark that drives fuel-core's `txpool_v2` and the
//! **parallel executor** together, multi-threaded, with a configurable worker
//! count and total block-gas limit. Unlike `tps_bench` it needs **no external
//! snapshot**: the chain state (funded user coins + deployed contracts) is built
//! programmatically at genesis, so it runs on any checkout with one command.
//!
//! The workload exercises the *contract-batching* path that the parallel
//! executor + lane scheduler care about: a Zipf-ish mix of single-contract
//! calls, multi-contract calls (2-3 contracts, the point of this branch) and
//! plain coin transfers. Scheduling in the parallel executor is driven by the
//! *contract inputs* a tx declares (the executor conservatively serialises any
//! two txs that share a contract input and parallelises disjoint ones); the lane
//! scheduler's Read/Write derivation likewise keys on inputs/outputs. So every
//! contract-call tx here carries a real `Input::contract` + matching
//! `Output::contract` and the script actually `CALL`s each contract.
//!
//! The deployed contract is a minimal callable stub (`ret ONE`) — enough to be
//! executed by the VM and to make the tx a *writer* of that contract for
//! scheduling purposes, without depending on any storage-slot semantics. That
//! keeps the harness stable; the scheduling structure (which is what we measure)
//! is identical to a heavier state-touching contract.
//!
//! ## Assembly level
//!
//! This drives the **full `FuelService`** (real `txpool_v2` service + real
//! parallel executor + block production via the client), assembled through the
//! `test_helpers` `TestSetupBuilder`. That is the highest-level assembly that
//! stays tractable and is the closest to production. The only source change
//! outside `benches/` is one additive `lane_scheduler` field on
//! `TestSetupBuilder` (there is no other way to flip the txpool flag through the
//! builder); default `false`, so existing callers are unchanged.
//!
//! ## Invocation
//!
//! ```text
//! cargo run --release -p fuel-core-benches --bin pool_executor_bench -- \
//!     --txs 2000 --contracts 16 --workers 4 --block-gas 30000000 --lane-scheduler off
//! cargo run --release -p fuel-core-benches --bin pool_executor_bench -- \
//!     --txs 2000 --contracts 16 --workers 4 --block-gas 30000000 --lane-scheduler on
//! ```
//!
//! The two runs above are one flag apart — that is the A/B toggle for the lane
//! scheduler. Everything else (seed, workload) is deterministic.

use clap::Parser;
use fuel_core::service::config::{
    ExecutorMode,
    Trigger,
};
use fuel_core_chain_config::{
    ChainConfig,
    CoinConfig,
    SnapshotMetadata,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_asm::{
        GTFArgs,
        RegId,
        op,
    },
    fuel_tx::{
        AssetId,
        ConsensusParameters,
        ContractId,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
    },
    fuel_vm::SecretKey,
};
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};
use std::time::{
    Duration,
    Instant,
};
use test_helpers::builder::{
    TestContext,
    TestSetupBuilder,
};

/// Load the committed local-testnet chainspec (config only, not a state
/// snapshot) relative to this crate's manifest so the binary works from any
/// working directory.
fn chain_config() -> ChainConfig {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bin/fuel-core/chainspec/local-testnet"
    );
    let metadata = SnapshotMetadata::read(path).expect("committed chainspec present");
    ChainConfig::from_snapshot_metadata(&metadata).expect("chainspec parses")
}

/// Size of one `Call` structure in the script data: contract id (32) + two
/// `Word` params (8 + 8).
const CALL_STRUCT_SIZE: u16 = 32 + 8 + 8;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "pool_executor_bench",
    about = "Drive txpool_v2 + the parallel executor together on a programmatic, \
             contract-heavy workload."
)]
struct Args {
    /// Number of parallel-executor worker threads.
    #[clap(long, alias = "threads", default_value = "4")]
    workers: usize,

    /// Total block gas limit (also used as the per-tx max-gas ceiling).
    #[clap(long, default_value = "30000000")]
    block_gas: u64,

    /// Enable the experimental lane scheduler in the txpool: `on`/`off`.
    #[clap(
        long,
        default_value = "off",
        value_parser = parse_on_off,
        action = clap::ArgAction::Set
    )]
    lane_scheduler: bool,

    /// Total number of workload transactions to generate and drain.
    #[clap(long, default_value = "2000")]
    txs: u64,

    /// Maximum number of blocks to produce. If omitted, run until the pool is
    /// drained.
    #[clap(long)]
    blocks: Option<u64>,

    /// Deterministic seed.
    #[clap(long, default_value = "2322")]
    seed: u64,

    /// Number of contracts to deploy at genesis.
    #[clap(long, default_value = "16")]
    contracts: usize,

    /// Fraction (0.0..=1.0) of workload txs that call at least one contract; the
    /// rest are plain coin transfers.
    #[clap(long, default_value = "0.85")]
    contract_tx_share: f64,

    /// Fraction (0.0..=1.0) of *contract* txs that touch 2-3 contracts (the
    /// multi-contract path) instead of a single contract.
    #[clap(long, default_value = "0.30")]
    multi_contract_share: f64,

    /// Per-tx script gas limit. Together with `--block-gas` this bounds how many
    /// txs land in each block.
    #[clap(long, default_value = "100000")]
    script_gas: u64,
}

fn parse_on_off(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        other => Err(format!("expected on/off, got `{other}`")),
    }
}

/// A minimal callable contract: `ret ONE`. Deployed at genesis; the workload
/// scripts `CALL` it.
fn stub_contract_code() -> Vec<u8> {
    op::ret(RegId::ONE).to_bytes().to_vec()
}

/// Zipf-ish contract picker: weight of rank `i` (0-based) is `1/(i+1)`, so the
/// low-index contracts are hot. Deterministic given `rng`.
fn pick_zipf(rng: &mut StdRng, k: usize, harmonic: f64) -> usize {
    debug_assert!(k > 0);
    let mut u = rng.r#gen::<f64>() * harmonic;
    for i in 0..k {
        u -= 1.0 / (i as f64 + 1.0);
        if u <= 0.0 {
            return i;
        }
    }
    k - 1
}

/// Build a contract-call tx that calls each contract in `targets` (1-3 of them).
/// Layout: coin input at index 0, then one contract input per target; a matching
/// contract output per target plus a change output. The script `CALL`s each
/// contract in turn.
fn build_contract_call_tx(
    rng: &mut StdRng,
    targets: &[ContractId],
    base_asset_id: AssetId,
    script_gas: u64,
) -> Transaction {
    let n = targets.len() as u16;

    // Script: for each target i, load ScriptData ptr, advance by i*CALL_STRUCT,
    // then CALL. Recomputing the pointer each iteration keeps it robust across
    // the call-frame save/restore.
    let mut script = Vec::new();
    for i in 0..n {
        script.push(op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData));
        if i > 0 {
            script.push(op::addi(0x10, 0x10, i * CALL_STRUCT_SIZE));
        }
        script.push(op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS));
    }
    script.push(op::ret(RegId::ONE));
    let script_bytes: Vec<u8> = script.into_iter().flat_map(|op| op.to_bytes()).collect();

    // Script data: one `Call` (contract id + two zero params) per target.
    let mut script_data = Vec::with_capacity(targets.len() * CALL_STRUCT_SIZE as usize);
    for c in targets {
        script_data.extend_from_slice(c.as_ref());
        script_data.extend_from_slice(&0u64.to_be_bytes());
        script_data.extend_from_slice(&0u64.to_be_bytes());
    }

    let mut builder = TransactionBuilder::script(script_bytes, script_data);
    builder
        .script_gas_limit(script_gas)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.r#gen(),
            u32::MAX as u64,
            base_asset_id,
            Default::default(),
        );
    // Contract inputs start at input index 1 (coin is 0).
    for c in targets {
        builder.add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            *c,
        ));
    }
    for i in 0..n {
        // Output contract's input_index points back at the contract input.
        builder.add_output(Output::contract(
            i + 1,
            Default::default(),
            Default::default(),
        ));
    }
    builder.add_output(Output::change(rng.r#gen(), 0, base_asset_id));
    builder.finalize_as_transaction()
}

/// Build a plain coin transfer (no contract input).
fn build_transfer_tx(
    rng: &mut StdRng,
    base_asset_id: AssetId,
    script_gas: u64,
) -> Transaction {
    let script_bytes = op::ret(RegId::ONE).to_bytes().to_vec();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    builder
        .script_gas_limit(script_gas)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.r#gen(),
            u32::MAX as u64,
            base_asset_id,
            Default::default(),
        )
        .add_output(Output::coin(rng.r#gen(), 1, base_asset_id))
        .add_output(Output::change(rng.r#gen(), 0, base_asset_id));
    builder.finalize_as_transaction()
}

/// Extract every coin input from `txs` and turn it into a genesis coin so the
/// tx is valid under utxo-validation (mirrors `tps_bench`).
fn genesis_coins_from(txs: &[Transaction]) -> Vec<CoinConfig> {
    txs.iter()
        .flat_map(|t| t.inputs().into_owned())
        .filter_map(|input| match input {
            Input::CoinSigned(CoinSigned {
                amount,
                owner,
                asset_id,
                utxo_id,
                tx_pointer,
                ..
            })
            | Input::CoinPredicate(CoinPredicate {
                amount,
                owner,
                asset_id,
                utxo_id,
                tx_pointer,
                ..
            }) => Some(CoinConfig {
                tx_id: *utxo_id.tx_id(),
                output_index: utxo_id.output_index(),
                tx_pointer_block_height: tx_pointer.block_height(),
                tx_pointer_tx_idx: tx_pointer.tx_index(),
                owner: owner.into(),
                amount,
                asset_id,
            }),
            _ => None,
        })
        .collect()
}

struct BlockResult {
    height: u32,
    txs_included: usize,
    declared_gas: u64,
    produce_ms: u128,
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let args = Args::parse();
    assert!(args.contracts > 0, "--contracts must be > 0");
    assert!(args.txs > 0, "--txs must be > 0");

    let mut rng = StdRng::seed_from_u64(args.seed);

    // Consensus parameters come from the committed local-testnet chainspec
    // (config, not state) — always present in the repo, no external snapshot.
    let chain_conf = chain_config();
    let base_asset_id = *chain_conf.consensus_parameters.base_asset_id();

    // --- Genesis contracts (deployed programmatically) ---------------------
    let mut builder = TestSetupBuilder::new(args.seed);
    let code = stub_contract_code();
    let mut contract_ids: Vec<ContractId> = Vec::with_capacity(args.contracts);
    for _ in 0..args.contracts {
        let (_salt, id) = builder.setup_contract(code.clone(), vec![], None);
        contract_ids.push(id);
    }

    // --- Workload generation (deterministic) -------------------------------
    let harmonic: f64 = (0..args.contracts).map(|i| 1.0 / (i as f64 + 1.0)).sum();
    let gen_start = Instant::now();
    let mut txs: Vec<Transaction> = Vec::with_capacity(args.txs as usize);
    let mut n_single = 0usize;
    let mut n_multi = 0usize;
    let mut n_transfer = 0usize;
    for _ in 0..args.txs {
        let is_contract = rng.r#gen::<f64>() < args.contract_tx_share;
        if !is_contract {
            txs.push(build_transfer_tx(&mut rng, base_asset_id, args.script_gas));
            n_transfer += 1;
            continue;
        }
        let is_multi =
            args.contracts >= 2 && rng.r#gen::<f64>() < args.multi_contract_share;
        if is_multi {
            // 2 or 3 distinct contracts.
            let want = if args.contracts >= 3 && rng.r#gen::<bool>() {
                3
            } else {
                2
            };
            let mut chosen: Vec<ContractId> = Vec::with_capacity(want);
            let mut guard = 0;
            while chosen.len() < want && guard < 64 {
                let idx = pick_zipf(&mut rng, args.contracts, harmonic);
                let id = contract_ids[idx];
                if !chosen.contains(&id) {
                    chosen.push(id);
                }
                guard += 1;
            }
            txs.push(build_contract_call_tx(
                &mut rng,
                &chosen,
                base_asset_id,
                args.script_gas,
            ));
            n_multi += 1;
        } else {
            let idx = pick_zipf(&mut rng, args.contracts, harmonic);
            txs.push(build_contract_call_tx(
                &mut rng,
                &[contract_ids[idx]],
                base_asset_id,
                args.script_gas,
            ));
            n_single += 1;
        }
    }
    let gen_ms = gen_start.elapsed().as_millis();

    // Fund every coin input at genesis.
    builder.initial_coins.extend(genesis_coins_from(&txs));

    // --- Service config ----------------------------------------------------
    builder.set_chain_config(chain_conf.clone());
    builder.trigger = Trigger::Never;
    builder.utxo_validation = true;
    builder.gas_limit = Some(args.block_gas);
    builder.block_size_limit = Some(u64::MAX);
    builder.number_threads_pool_verif = args.workers.max(1);
    builder.executor_parallel_worker_count = args.workers.max(1);
    builder.max_txs = (args.txs as usize).saturating_add(args.contracts) + 16;
    builder.lane_scheduler = args.lane_scheduler;
    #[cfg(feature = "parallel-executor")]
    {
        builder.executor_mode = ExecutorMode::Parallel;
        builder.executor_metrics = true;
    }
    #[cfg(not(feature = "parallel-executor"))]
    {
        let _ = ExecutorMode::Normal;
        eprintln!(
            "WARNING: built without the `parallel-executor` feature; running the \
             sequential executor. Rebuild with the default features for the \
             parallel path."
        );
    }

    print_header(&args, &contract_ids, n_single, n_multi, n_transfer, gen_ms);

    // --- Run ---------------------------------------------------------------
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.workers.max(4) + 2)
        .enable_all()
        .build()
        .unwrap();
    let _enter = rt.enter();

    rt.block_on(run(
        builder,
        txs,
        args,
        chain_conf.consensus_parameters.clone(),
    ));
}

async fn run(
    mut builder: TestSetupBuilder,
    txs: Vec<Transaction>,
    args: Args,
    consensus_params: ConsensusParameters,
) {
    let total_txs = txs.len();
    let TestContext { srv, client, .. } = builder.finalize().await;

    // Insert the whole workload and wait for verification to settle it into the
    // pool.
    srv.shared
        .txpool_shared_state
        .try_insert(txs)
        .expect("insert into txpool");

    wait_until(
        || srv.shared.txpool_shared_state.latest_stats().tx_count as usize >= total_txs,
        Duration::from_secs(120),
        "all txs verified into pool",
    )
    .await;
    let pending_at_start = srv.shared.txpool_shared_state.latest_stats().tx_count;
    println!("\nPool loaded: {pending_at_start} executable txs. Producing blocks...\n");

    let mut results: Vec<BlockResult> = Vec::new();
    let run_start = Instant::now();
    let max_blocks = args.blocks.unwrap_or(u64::MAX);
    let mut height: u32 = 0;

    while (results.len() as u64) < max_blocks {
        let remaining = srv.shared.txpool_shared_state.latest_stats().tx_count;
        if remaining == 0 {
            break;
        }
        height += 1;

        let produce_start = Instant::now();
        client.produce_blocks(1, None).await.expect("produce block");
        let produce_ms = produce_start.elapsed().as_millis();

        let block = srv
            .shared
            .database
            .on_chain()
            .latest_view()
            .unwrap()
            .get_sealed_block_by_height(&height.into())
            .unwrap()
            .expect("produced block present");
        let block_txs = block.entity.transactions();
        // Exclude the coinbase mint tx from the count.
        let included = block_txs.iter().filter(|t| !t.is_mint()).count();
        let declared_gas: u64 = block_txs
            .iter()
            .filter(|t| !t.is_mint())
            .filter_map(|t| t.max_gas(&consensus_params).ok())
            .sum();

        let res = BlockResult {
            height,
            txs_included: included,
            declared_gas,
            produce_ms,
        };
        print_block(&res);
        results.push(res);

        if included == 0 {
            // Nothing fit / nothing left that can be scheduled — avoid spinning.
            let still = srv.shared.txpool_shared_state.latest_stats().tx_count;
            if still > 0 {
                println!(
                    "  (produced an empty block with {still} txs still pending — \
                     stopping to avoid a spin)"
                );
            }
            break;
        }
    }

    let wall = run_start.elapsed();
    print_summary(&results, total_txs, wall, args.lane_scheduler);
    print_executor_metrics();

    // Clean shutdown so the temp rocksdb is released.
    let _ = srv.send_stop_signal_and_await_shutdown().await;
}

async fn wait_until<F: Fn() -> bool>(cond: F, timeout: Duration, what: &str) {
    let start = Instant::now();
    loop {
        if cond() {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timed out waiting for: {what}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn print_header(
    args: &Args,
    contract_ids: &[ContractId],
    n_single: usize,
    n_multi: usize,
    n_transfer: usize,
    gen_ms: u128,
) {
    println!("======================================================================");
    println!(" pool_executor_bench  (txpool_v2 + parallel executor)");
    println!("======================================================================");
    println!(" workers (executor)     : {}", args.workers);
    println!(" block gas limit        : {}", args.block_gas);
    println!(" per-tx script gas      : {}", args.script_gas);
    println!(
        " lane scheduler         : {}",
        if args.lane_scheduler { "ON" } else { "off" }
    );
    println!(" seed                   : {}", args.seed);
    println!(" contracts deployed     : {}", contract_ids.len());
    println!(
        " max blocks             : {}",
        args.blocks
            .map(|b| b.to_string())
            .unwrap_or_else(|| "until drained".to_string())
    );
    println!(
        " workload               : {} txs generated in {gen_ms} ms",
        args.txs
    );
    println!(
        "   single-contract      : {n_single}\n   multi-contract (2-3) : {n_multi}\n   plain transfers      : {n_transfer}"
    );
    println!("----------------------------------------------------------------------");
}

fn print_block(r: &BlockResult) {
    println!(
        " block {:>4} | txs {:>5} | declared_gas {:>12} | produce {:>6} ms",
        r.height, r.txs_included, r.declared_gas, r.produce_ms
    );
}

fn print_summary(
    results: &[BlockResult],
    total_txs: usize,
    wall: Duration,
    lane_scheduler: bool,
) {
    let included: usize = results.iter().map(|r| r.txs_included).sum();
    let gas: u64 = results.iter().map(|r| r.declared_gas).sum();
    let produce_ms: u128 = results.iter().map(|r| r.produce_ms).sum();
    let secs = wall.as_secs_f64().max(f64::MIN_POSITIVE);
    let produce_secs = (produce_ms as f64 / 1000.0).max(f64::MIN_POSITIVE);
    println!("----------------------------------------------------------------------");
    println!(
        " SUMMARY  (lane scheduler {})",
        if lane_scheduler { "ON" } else { "off" }
    );
    println!(" blocks produced        : {}", results.len());
    println!(" txs included           : {included} / {total_txs}");
    println!(" total declared gas     : {gas}");
    println!(" wall clock             : {:.3} s", secs);
    println!(" sum(produce) time      : {:.3} s", produce_secs);
    println!(" TPS (incl / wall)      : {:.1}", included as f64 / secs);
    println!(
        " TPS (incl / produce)   : {:.1}",
        included as f64 / produce_secs
    );
    println!(" gas/s (produce)        : {:.0}", gas as f64 / produce_secs);
    println!("======================================================================");
}

fn print_executor_metrics() {
    if let Ok(metrics) = fuel_core_metrics::encode_metrics() {
        let mut any = false;
        for line in metrics.lines().filter(|line| {
            !line.starts_with('#')
                && line.starts_with("parallel_executor_")
                && (line.contains("non_empty_batches")
                    || line.contains("total_gas_used")
                    || line.contains("batch_total_ms")
                    || line.contains("pool_ask"))
        }) {
            if !any {
                println!("\nparallel-executor metrics (cumulative):");
                any = true;
            }
            println!("  {line}");
        }
    }
}
