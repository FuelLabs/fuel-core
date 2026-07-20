//! `fuel-core validate-benchmark` — re-validate a range of already-produced
//! blocks and time each validation.
//!
//! The command owns two databases:
//! * `--db-path` — the STATE database, a caller-prepared COPY of the pre-run
//!   seed database. Every validated block's changes (plus the block itself)
//!   are committed into it, so block `h + 1` validates against the state left
//!   by block `h`, exactly like a syncing node.
//! * `--blocks-db-path` — the block SOURCE database (a finished run's
//!   database). Only read (blocks, transactions, seals), but RocksDB may still
//!   touch its files on open (WAL replay) — pass a copy if the original must
//!   stay pristine.
//!
//! Modes: `sequential` re-validates through the production (upgradable)
//! executor; `parallel` re-validates through the parallel executor's new
//! `validate` path (requires the `parallel-executor` feature).

use anyhow::{
    Context,
    anyhow,
    bail,
};
use clap::{
    Parser,
    ValueEnum,
};
use fuel_core::{
    combined_database::CombinedDatabase,
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::{
            ColumnsPolicy,
            DatabaseConfig,
        },
    },
};
use fuel_core_importer::ports::{
    DatabaseTransaction as _,
    ImporterDatabase as _,
};
use fuel_core_storage::{
    StorageAsRef,
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::{
        AtomicView,
        ConflictPolicy,
        StorageChanges,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::{
        SealedBlock,
        block::Block,
    },
    fuel_tx::UniqueIdentifier,
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::executor::{
        TransactionExecutionResult,
        ValidationResult,
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::Instant,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// The production sequential (upgradable) executor.
    Sequential,
    /// The parallel executor's validation path (requires the
    /// `parallel-executor` feature).
    Parallel,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Sequential => f.write_str("sequential"),
            Mode::Parallel => f.write_str("parallel"),
        }
    }
}

/// Re-validate a range of blocks from a run database, timing each block.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    /// Path to the STATE database (a COPY of the pre-run seed database);
    /// validation changes are committed into it block by block.
    #[clap(long = "db-path")]
    pub database_path: PathBuf,

    /// Path to the database holding the blocks to validate (a finished run's
    /// database).
    #[clap(long = "blocks-db-path")]
    pub blocks_database_path: PathBuf,

    /// First block height to validate. Defaults to the state database's
    /// latest height + 1.
    #[clap(long)]
    pub from: Option<u32>,

    /// Last block height to validate (inclusive). Defaults to the blocks
    /// database's latest height.
    #[clap(long)]
    pub to: Option<u32>,

    /// Which executor validates the blocks.
    #[clap(long, value_enum, default_value = "sequential")]
    pub mode: Mode,

    /// Worker count for `--mode parallel`.
    #[clap(long, default_value = "4")]
    pub workers: usize,

    /// Enable full UTXO validation (signature/predicate + coin existence
    /// checks) during re-execution, matching a node run with
    /// `--utxo-validation`.
    #[clap(long = "utxo-validation", default_value = "false")]
    pub utxo_validation: bool,

    /// `--mode parallel` only: plan on ACTUAL per-transaction gas instead of
    /// declared `max_gas`. The actual gas is obtained by a throwaway
    /// validation pass (execution is deterministic, so it yields the real
    /// gas), then fed to the measured pass via the planner. The executor's
    /// carried batch-overhead estimate is snapshotted and restored around the
    /// throwaway pass, so the measured pass plans exactly as a clean
    /// single-pass validation would. Roughly doubles the parallel work per
    /// block. Run once with and once without this flag to compare.
    #[clap(long = "use-actual-gas", default_value = "false")]
    pub use_actual_gas: bool,
}

fn open_database(path: &PathBuf) -> anyhow::Result<CombinedDatabase> {
    CombinedDatabase::open(
        path,
        StateRewindPolicy::NoRewind,
        DatabaseConfig {
            cache_capacity: Some(256 * 1024 * 1024),
            max_fds: -1,
            columns_policy: ColumnsPolicy::Lazy,
        },
    )
    .map_err(Into::<anyhow::Error>::into)
    .with_context(|| format!("failed to open combined database at {path:?}"))
}

/// Load the full block + its seal for `height` from the blocks database.
fn load_sealed_block(
    blocks_db: &CombinedDatabase,
    height: BlockHeight,
) -> anyhow::Result<SealedBlock> {
    let view = blocks_db.on_chain().latest_view()?;
    let compressed = view
        .storage::<FuelBlocks>()
        .get(&height)?
        .ok_or_else(|| anyhow!("block {height} not found in the blocks database"))?
        .into_owned();
    let mut transactions = Vec::with_capacity(compressed.transactions().len());
    for tx_id in compressed.transactions() {
        let tx = view
            .storage::<Transactions>()
            .get(tx_id)?
            .ok_or_else(|| anyhow!("transaction {tx_id} of block {height} not found"))?
            .into_owned();
        transactions.push(tx);
    }
    let block = compressed.uncompress(transactions);
    let consensus = view
        .storage::<SealedBlockConsensus>()
        .get(&height)?
        .ok_or_else(|| anyhow!("seal for block {height} not found"))?
        .into_owned();
    Ok(SealedBlock {
        entity: block,
        consensus,
    })
}

/// The chain id the block was produced under (needed to compress/store it).
fn chain_id_for_block(
    state_db: &CombinedDatabase,
    block: &Block,
) -> anyhow::Result<ChainId> {
    let version = block.header().consensus_parameters_version();
    let view = state_db.on_chain().latest_view()?;
    let params = view
        .storage::<ConsensusParametersVersions>()
        .get(&version)?
        .ok_or_else(|| {
            anyhow!("consensus parameters version {version} not found in state db")
        })?
        .into_owned();
    Ok(params.chain_id())
}

/// Commit a validated block into the state database exactly like the importer
/// does: the validation changes plus the block/seal/transactions records, as
/// one atomic commit (with the database height update).
fn commit_validated_block(
    on_chain: &mut fuel_core::database::Database,
    chain_id: &ChainId,
    sealed_block: &SealedBlock,
    changes: fuel_core_storage::transactional::Changes,
) -> anyhow::Result<()> {
    let view = on_chain.latest_view()?;
    let mut db_tx =
        StorageTransaction::transaction(view, ConflictPolicy::Overwrite, changes);
    db_tx
        .store_new_block(chain_id, sealed_block)
        .map_err(|e| anyhow!("store validated block: {e}"))?;
    let changes = db_tx.into_changes();
    on_chain
        .commit_changes(StorageChanges::Changes(changes))
        .map_err(|e| anyhow!("commit validated block changes: {e}"))?;
    Ok(())
}

/// The block's per-transaction actual gas used, indexed by each transaction's
/// position in the block (mirrors the importer's `transactions_gas_used`), to
/// feed the planner as the actual-gas hint.
fn gas_used_in_block_order(
    chain_id: &ChainId,
    sealed_block: &SealedBlock,
    result: &ValidationResult,
) -> Vec<u64> {
    let by_id: HashMap<_, u64> = result
        .tx_status
        .iter()
        .map(|status| {
            let gas = match &status.result {
                TransactionExecutionResult::Success { total_gas, .. }
                | TransactionExecutionResult::Failed { total_gas, .. } => *total_gas,
            };
            (status.id, gas)
        })
        .collect();
    sealed_block
        .entity
        .transactions()
        .iter()
        .map(|tx| by_id.get(&tx.id(chain_id)).copied().unwrap_or(0))
        .collect()
}

fn used_gas_of(result: &ValidationResult) -> u64 {
    result
        .tx_status
        .iter()
        .map(|status| match &status.result {
            TransactionExecutionResult::Success { total_gas, .. }
            | TransactionExecutionResult::Failed { total_gas, .. } => *total_gas,
        })
        .sum()
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    let state_db = open_database(&command.database_path)?;
    let blocks_db = open_database(&command.blocks_database_path)?;
    // Clones share the same underlying RocksDB; this handle receives commits.
    let mut state_on_chain = state_db.on_chain().clone();

    let state_height = state_db
        .on_chain()
        .latest_block_height()?
        .map(u32::from)
        .ok_or_else(|| anyhow!("state database has no blocks"))?;
    let blocks_height = blocks_db
        .on_chain()
        .latest_block_height()?
        .map(u32::from)
        .ok_or_else(|| anyhow!("blocks database has no blocks"))?;

    let from = command
        .from
        .unwrap_or_else(|| state_height.saturating_add(1));
    let to = command.to.unwrap_or(blocks_height);
    if from > to {
        bail!(
            "empty validation range: from={from} > to={to} \
             (state height {state_height}, blocks height {blocks_height})"
        );
    }
    println!(
        "[validate-benchmark] mode={} workers={} utxo_validation={} \
         range={from}..={to} state_height={state_height}",
        command.mode, command.workers, command.utxo_validation
    );

    // Executors are built once; both observe the state database as it advances
    // (they take latest views per validation).
    let sequential_executor = match command.mode {
        Mode::Sequential => {
            Some(fuel_core::upgradable_executor::executor::Executor::new(
                state_db.on_chain().clone(),
                state_db.relayer().clone(),
                fuel_core::upgradable_executor::config::Config {
                    forbid_unauthorized_inputs_default: command.utxo_validation,
                    forbid_fake_utxo_default: command.utxo_validation,
                    allow_syscall: false,
                    native_executor_version: None,
                    allow_historical_execution: false,
                },
            ))
        }
        Mode::Parallel => None,
    };

    #[cfg(feature = "parallel-executor")]
    let mut parallel_executor = match command.mode {
        Mode::Parallel => {
            let worker_count = std::num::NonZeroUsize::new(command.workers)
                .ok_or_else(|| anyhow!("--workers must be > 0"))?;
            Some(
                fuel_core::parallel_executor::executor::Executor::new(
                    state_db.on_chain().clone(),
                    state_db.relayer().clone(),
                    fuel_core_executor::executor::TransparentPreconfirmationSender,
                    fuel_core::parallel_executor::config::Config {
                        worker_count,
                        worker_count_policy:
                            fuel_core::parallel_executor::config::WorkerCountPolicy::StaticMax,
                        // Emit the per-block `block_summary` time-spend
                        // decomposition (window / worker busy / ask / merge /
                        // idle) — the whole point of this benchmark is stage
                        // attribution. Visible with
                        // RUST_LOG=parallel_executor::block_summary=info.
                        metrics: true,
                        utxo_validation: command.utxo_validation,
                    },
                )
                .context("build parallel executor")?,
            )
        }
        Mode::Sequential => None,
    };
    #[cfg(not(feature = "parallel-executor"))]
    if command.mode == Mode::Parallel {
        bail!(
            "this binary was built without the `parallel-executor` feature; \
             `--mode parallel` is unavailable"
        );
    }

    let mut total_wall_ms = 0f64;
    let mut max_wall_ms = 0f64;
    let mut total_txs: u64 = 0;
    let mut blocks: u64 = 0;

    for height in from..=to {
        let height: BlockHeight = height.into();
        let sealed_block = load_sealed_block(&blocks_db, height)
            .with_context(|| format!("load block {height}"))?;
        let chain_id = chain_id_for_block(&state_db, &sealed_block.entity)?;
        let tx_count = sealed_block.entity.transactions().len();

        let (result, changes, wall_ms, used_hint) = match command.mode {
            Mode::Sequential => {
                let executor = sequential_executor
                    .as_ref()
                    .expect("built for sequential mode; qed");
                let started = Instant::now();
                let (result, changes) = executor
                    .validate(&sealed_block.entity)
                    .map_err(|e| anyhow!("sequential validation failed: {e}"))
                    .with_context(|| format!("block {height}"))?
                    .into();
                let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
                (result, changes, wall_ms, false)
            }
            Mode::Parallel => {
                #[cfg(feature = "parallel-executor")]
                {
                    let executor = parallel_executor
                        .as_mut()
                        .expect("built for parallel mode; qed");
                    if command.use_actual_gas {
                        // Throwaway pass to obtain the block's ACTUAL gas
                        // (execution is deterministic). Snapshot and restore
                        // the carried batch-overhead estimate around it so the
                        // measured pass below plans exactly as a clean
                        // single-pass validation would.
                        let saved_overhead = executor.validation_overhead();
                        let (probe, _changes) = executor
                            .validate(&sealed_block.entity)
                            .await
                            .map_err(|e| anyhow!("gas-probe validation failed: {e}"))
                            .with_context(|| format!("block {height}"))?
                            .into();
                        executor.set_validation_overhead(saved_overhead);
                        let actual_gas =
                            gas_used_in_block_order(&chain_id, &sealed_block, &probe);
                        // Measured pass: feed the planner the actual gas.
                        let started = Instant::now();
                        let (result, changes) = executor
                            .validate_with_gas(&sealed_block.entity, Some(&actual_gas))
                            .await
                            .map_err(|e| anyhow!("actual-gas validation failed: {e}"))
                            .with_context(|| format!("block {height}"))?
                            .into();
                        let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
                        (result, changes, wall_ms, true)
                    } else {
                        let started = Instant::now();
                        let (result, changes) = executor
                            .validate(&sealed_block.entity)
                            .await
                            .map_err(|e| anyhow!("parallel validation failed: {e}"))
                            .with_context(|| format!("block {height}"))?
                            .into();
                        let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
                        (result, changes, wall_ms, false)
                    }
                }
                #[cfg(not(feature = "parallel-executor"))]
                unreachable!("rejected above")
            }
        };

        let commit_started = Instant::now();
        commit_validated_block(&mut state_on_chain, &chain_id, &sealed_block, changes)
            .with_context(|| format!("commit block {height}"))?;
        let commit_ms = commit_started.elapsed().as_secs_f64() * 1000.0;

        let gas_used = used_gas_of(&result);
        println!(
            "[validate] mode={} height={} txs={} wall_ms={:.3} commit_ms={:.3} \
             gas_used={} gas_plan={}",
            command.mode,
            u32::from(height),
            tx_count,
            wall_ms,
            commit_ms,
            gas_used,
            if used_hint { "actual" } else { "max_gas" },
        );

        total_wall_ms += wall_ms;
        max_wall_ms = max_wall_ms.max(wall_ms);
        total_txs += tx_count as u64;
        blocks += 1;
    }

    let avg_ms = total_wall_ms / blocks as f64;
    let tps = if total_wall_ms > 0.0 {
        (total_txs as f64) / (total_wall_ms / 1000.0)
    } else {
        0.0
    };
    println!(
        "[validate-summary] mode={} gas_plan={} blocks={} total_ms={:.3} \
         avg_ms={:.3} max_ms={:.3} total_txs={} tps={:.1}",
        command.mode,
        if command.use_actual_gas {
            "actual"
        } else {
            "max_gas"
        },
        blocks,
        total_wall_ms,
        avg_ms,
        max_wall_ms,
        total_txs,
        tps,
    );

    Ok(())
}
