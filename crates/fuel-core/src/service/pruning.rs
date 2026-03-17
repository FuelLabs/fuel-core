use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
        Database,
    },
    graphql_api::storage::{
        blocks::FuelBlockIdsToHeights,
        old::{
            OldFuelBlockConsensus,
            OldFuelBlocks,
            OldTransactions,
        },
        transactions::{
            OwnedTransactionIndexKey,
            OwnedTransactions,
            TransactionStatuses,
        },
    },
    service::config::HistoryRetentionConfig,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
};
use fuel_core_storage::{
    StorageAsMut,
    StorageAsRef,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::{
        AtomicView,
        WriteTransaction,
    },
};
use fuel_core_types::{
    fuel_tx::{
        Input,
        Output,
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
        },
    },
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
    tai64::Tai64,
};
use futures::StreamExt;
use std::time::Duration;

/// The pruning service task that runs after initialization.
pub struct PruningTask {
    config: HistoryRetentionConfig,
    on_chain_db: Database<OnChain>,
    off_chain_db: Database<OffChain>,
    block_import_stream: BoxStream<SharedImportResult>,
    earliest_retained_height: BlockHeight,
}

/// The initialization task for the pruning service.
pub struct PruningInitTask {
    pub config: HistoryRetentionConfig,
    pub on_chain_db: Database<OnChain>,
    pub off_chain_db: Database<OffChain>,
    pub block_import_stream: BoxStream<SharedImportResult>,
    pub earliest_retained_height: BlockHeight,
}

/// Compute the cutoff block height based on the latest block's timestamp and the
/// retention duration. Returns `None` if there's no block or the retention window
/// covers all existing blocks.
pub fn compute_cutoff_height(
    on_chain_db: &Database<OnChain>,
    retention_duration: Duration,
) -> anyhow::Result<Option<BlockHeight>> {
    let view = on_chain_db.latest_view()?;
    let latest_block = view.get_current_block()?;

    let Some(latest_block) = latest_block else {
        return Ok(None);
    };

    let latest_time = latest_block.header().time();
    let retention_secs = retention_duration.as_secs();

    // Compute cutoff time: latest_time - retention_duration
    let cutoff_tai64 = Tai64(latest_time.0.saturating_sub(retention_secs));

    // If the cutoff is at or before the epoch, nothing to prune
    if cutoff_tai64.0 <= Tai64::UNIX_EPOCH.0 {
        return Ok(None);
    }

    let latest_height: u32 = (*latest_block.header().height()).into();

    // Scan backward from latest to find the cutoff height
    for h in (0..=latest_height).rev() {
        let height = BlockHeight::new(h);
        if let Some(block) = view.storage::<FuelBlocks>().get(&height)? {
            if block.header().time() < cutoff_tai64 {
                return Ok(Some(height));
            }
        }
    }

    Ok(None)
}

/// Collect owner addresses from a transaction's inputs and outputs.
fn collect_owners(
    on_chain_db: &Database<OnChain>,
    tx_id: &fuel_core_types::fuel_tx::Bytes32,
) -> Vec<fuel_core_types::fuel_tx::Address> {
    let view = match on_chain_db.latest_view() {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    use fuel_core_types::blockchain::transaction::TransactionExt;

    let tx = match view.storage::<Transactions>().get(tx_id) {
        Ok(Some(tx)) => tx.into_owned(),
        _ => return vec![],
    };

    let mut owners: Vec<fuel_core_types::fuel_tx::Address> = vec![];
    for input in tx.inputs().iter() {
        if let Input::CoinSigned(CoinSigned { owner, .. })
        | Input::CoinPredicate(CoinPredicate { owner, .. }) = input
        {
            owners.push((*owner).into());
        }
    }
    for output in tx.outputs().iter() {
        match output {
            Output::Coin { to, .. }
            | Output::Change { to, .. }
            | Output::Variable { to, .. } => {
                owners.push((*to).into());
            }
            _ => {}
        }
    }
    owners.sort();
    owners.dedup();
    owners
}

/// Prune a single block at the given height.
/// Uses write transactions for batched mutations, bypassing height validation.
pub fn prune_block_at_height(
    on_chain_db: &mut Database<OnChain>,
    off_chain_db: &mut Database<OffChain>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    // 1. Look up the block to get tx ids and block_id
    let view = on_chain_db.latest_view()?;
    let block = view.storage::<FuelBlocks>().get(&height)?;

    if let Some(block) = block {
        let block = block.into_owned();
        let block_id = block.id();
        let tx_ids: Vec<_> = block.transactions().to_vec();
        tracing::trace!(
            "Pruning block at height {} ({} transactions)",
            u32::from(height),
            tx_ids.len()
        );
        drop(view);

        // 2. Collect owners for OwnedTransactions removal (must happen before on-chain deletion)
        let mut all_owners: Vec<(fuel_core_types::fuel_tx::Address, u16)> = Vec::new();
        for (tx_idx, tx_id) in tx_ids.iter().enumerate() {
            let owners = collect_owners(on_chain_db, tx_id);
            for owner in owners {
                all_owners.push((owner, tx_idx as u16));
            }
        }

        // 3. Delete off-chain data first (before on-chain data is removed)
        {
            let mut tx = off_chain_db.write_transaction();
            for tx_id in &tx_ids {
                let _ = tx.storage_as_mut::<TransactionStatuses>().take(tx_id);
                let _ = tx.storage_as_mut::<OldTransactions>().take(tx_id);
            }
            // Remove OwnedTransactions entries
            for (owner, tx_idx) in &all_owners {
                let key = OwnedTransactionIndexKey::new(owner, height, *tx_idx);
                let _ = tx.storage_as_mut::<OwnedTransactions>().take(&key);
            }
            let _ = tx.storage_as_mut::<FuelBlockIdsToHeights>().take(&block_id);
            let _ = tx.storage_as_mut::<OldFuelBlocks>().take(&height);
            let _ = tx.storage_as_mut::<OldFuelBlockConsensus>().take(&height);
            let changes = tx.into_changes();
            off_chain_db.commit_changes_without_height_check(changes)?;
        }

        // 4. Delete on-chain data, bypassing height validation
        {
            let mut tx = on_chain_db.write_transaction();
            for tx_id in &tx_ids {
                let _ = tx.storage_as_mut::<Transactions>().take(tx_id);
            }
            let _ = tx.storage_as_mut::<FuelBlocks>().take(&height);
            let _ = tx.storage_as_mut::<SealedBlockConsensus>().take(&height);
            let changes = tx.into_changes();
            on_chain_db.commit_changes_without_height_check(changes)?;
        }
    } else {
        drop(view);
        tracing::trace!(
            "Block at height {} already pruned, cleaning up off-chain remnants",
            u32::from(height)
        );
        // Block already pruned — try off-chain cleanup
        let mut tx = off_chain_db.write_transaction();
        let _ = tx.storage_as_mut::<OldFuelBlocks>().take(&height);
        let _ = tx.storage_as_mut::<OldFuelBlockConsensus>().take(&height);
        let changes = tx.into_changes();
        off_chain_db.commit_changes_without_height_check(changes)?;
    }

    Ok(())
}

/// Prune all blocks in the range [from, to) (exclusive of `to`).
pub fn prune_blocks_range(
    on_chain_db: &mut Database<OnChain>,
    off_chain_db: &mut Database<OffChain>,
    from: BlockHeight,
    to: BlockHeight,
) -> anyhow::Result<()> {
    let from_u32: u32 = from.into();
    let to_u32: u32 = to.into();

    // Never prune the genesis block (height 0) — it is needed for
    // startup initialization and peer handshake.
    let start = if from_u32 == 0 { 1 } else { from_u32 };

    let total = to_u32.saturating_sub(start);
    let mut pruned: u32 = 0;

    for h in start..to_u32 {
        prune_block_at_height(on_chain_db, off_chain_db, BlockHeight::new(h))?;
        pruned += 1;
        if pruned % 10_000 == 0 {
            tracing::info!(
                "Pruning progress: {}/{} blocks ({:.1}%)",
                pruned,
                total,
                (pruned as f64 / total as f64) * 100.0
            );
        }
    }

    Ok(())
}

/// Perform startup pruning. Returns the new earliest retained height.
pub fn startup_prune(
    config: &HistoryRetentionConfig,
    on_chain_db: &mut Database<OnChain>,
    off_chain_db: &mut Database<OffChain>,
    earliest_retained: BlockHeight,
) -> anyhow::Result<BlockHeight> {
    let cutoff = compute_cutoff_height(on_chain_db, config.retention_duration)?;

    let Some(cutoff_height) = cutoff else {
        let earliest_u32: u32 = earliest_retained.into();
        tracing::info!(
            "No startup pruning needed. Earliest retained height: {}",
            earliest_u32
        );
        return Ok(earliest_retained);
    };

    let cutoff_u32: u32 = cutoff_height.into();
    let earliest_u32: u32 = earliest_retained.into();

    if cutoff_u32 <= earliest_u32 {
        tracing::info!(
            "No startup pruning needed. Earliest retained height: {}",
            earliest_u32
        );
        return Ok(earliest_retained);
    }

    tracing::info!(
        "Pruning historical data from height {} to {}",
        earliest_u32,
        cutoff_u32
    );

    prune_blocks_range(on_chain_db, off_chain_db, earliest_retained, cutoff_height)?;

    tracing::info!(
        "Startup pruning complete. Pruned {} blocks (height {} to {}). Earliest retained height: {}",
        cutoff_u32 - earliest_u32,
        earliest_u32,
        cutoff_u32,
        cutoff_u32
    );

    Ok(cutoff_height)
}

#[async_trait::async_trait]
impl RunnableService for PruningInitTask {
    const NAME: &'static str = "HistoryPruning";
    type SharedData = ();
    type Task = PruningTask;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(
        self,
        _watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        tracing::info!(
            "History pruning enabled with retention duration of {:?}",
            self.config.retention_duration
        );
        Ok(PruningTask {
            config: self.config,
            on_chain_db: self.on_chain_db,
            off_chain_db: self.off_chain_db,
            block_import_stream: self.block_import_stream,
            earliest_retained_height: self.earliest_retained_height,
        })
    }
}

impl RunnableTask for PruningTask {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }

            result = self.block_import_stream.next() => {
                match result {
                    Some(_import_result) => {
                        // Compute new cutoff
                        if let Ok(Some(cutoff_height)) = compute_cutoff_height(
                            &self.on_chain_db,
                            self.config.retention_duration,
                        ) {
                            let cutoff_u32: u32 = cutoff_height.into();
                            let earliest_u32: u32 = self.earliest_retained_height.into();

                            if cutoff_u32 > earliest_u32 {
                                if let Err(e) = prune_blocks_range(
                                    &mut self.on_chain_db,
                                    &mut self.off_chain_db,
                                    self.earliest_retained_height,
                                    cutoff_height,
                                ) {
                                    tracing::error!("Pruning error: {}", e);
                                    return TaskNextAction::ErrorContinue(e);
                                }
                                self.earliest_retained_height = cutoff_height;
                                tracing::info!(
                                    "Pruned {} blocks (height {} to {}). Earliest retained height: {}",
                                    cutoff_u32 - earliest_u32,
                                    earliest_u32,
                                    cutoff_u32,
                                    cutoff_u32,
                                );
                            }
                        }

                        TaskNextAction::Continue
                    }
                    None => TaskNextAction::Stop,
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Create a new pruning service.
pub fn new_service(
    config: HistoryRetentionConfig,
    on_chain_db: Database<OnChain>,
    off_chain_db: Database<OffChain>,
    block_import_stream: BoxStream<SharedImportResult>,
    earliest_retained_height: BlockHeight,
) -> ServiceRunner<PruningInitTask> {
    ServiceRunner::new(PruningInitTask {
        config,
        on_chain_db,
        off_chain_db,
        block_import_stream,
        earliest_retained_height,
    })
}
