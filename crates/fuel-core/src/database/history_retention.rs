//! Offline history retention prune (DEVOPS-1916) — delete scope **B**.
//!
//! Deletes GraphQL/offchain historical data **and** aged onchain `FuelBlocks` /
//! `Transactions` / sealed consensus older than the retention window.
//!
//! Keeps live validation state: Coins / Contracts* / Messages / UTXO.
//! Keeps `ProcessedTransactions` (dup detect) and block-merkle leaf data so the
//! tip merkle root stays valid (`FuelBlocks` is Merklized and forbids Storage
//! remove — body bytes are deleted via raw column ops).
//!
//! Does **not** replace `--db-prune` (full RocksDB destroy + regenesis).
//! Full Archive nodes must leave history retention disabled.

use crate::{
    database::{
        Database,
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
    },
    fuel_core_graphql_api::storage::{
        blocks::FuelBlockIdsToHeights,
        old::{
            OldFuelBlockConsensus,
            OldFuelBlocks,
            OldTransactions,
        },
        transactions::{
            OwnedTransactions,
            TransactionStatuses,
        },
    },
};
use fuel_core_storage::{
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    kv_store::{
        StorageColumn,
        WriteOperation,
    },
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::{
        Changes,
        ConflictPolicy,
        StorageChanges,
        StorageTransaction,
    },
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    tai64::Tai64,
};
use std::time::Duration;

/// How many block heights to delete per RocksDB commit.
const DELETE_BATCH_HEIGHTS: usize = 64;

/// Summary of an offline history prune (scope B).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPruneReport {
    /// Exclusive end of pruned height range: deleted heights are `< prune_before`.
    pub prune_before: Option<BlockHeight>,
    pub deleted_transaction_statuses: u64,
    pub deleted_block_id_index: u64,
    pub deleted_owned_transactions: u64,
    pub deleted_old_blocks: u64,
    pub deleted_old_consensus: u64,
    pub deleted_old_transactions: u64,
    pub deleted_onchain_transactions: u64,
    pub deleted_onchain_blocks: u64,
    pub deleted_onchain_consensus: u64,
}

/// Format a retention duration for operator-facing / GraphQL errors.
///
/// Prefers day/hour suffix when the value is an exact multiple; otherwise seconds.
pub fn format_retention_duration(retention: Duration) -> String {
    let secs = retention.as_secs();
    const DAY: u64 = 86_400;
    const HOUR: u64 = 3_600;
    if secs > 0 && secs.checked_rem(DAY) == Some(0) {
        let days = secs.checked_div(DAY).expect("DAY is non-zero");
        format!("{days}d")
    } else if secs > 0 && secs.checked_rem(HOUR) == Some(0) {
        let hours = secs.checked_div(HOUR).expect("HOUR is non-zero");
        format!("{hours}h")
    } else {
        format!("{secs}s")
    }
}

/// Cutoff timestamp: entries with `time < cutoff` are outside the retention window.
pub fn cutoff_time(now: Tai64, retention: Duration) -> Tai64 {
    Tai64(now.0.saturating_sub(retention.as_secs()))
}

/// Latest on-chain block height from stored `FuelBlocks`, if any.
pub fn on_chain_tip_height(
    on_chain: &Database<OnChain>,
) -> StorageResult<Option<BlockHeight>> {
    on_chain
        .iter_all_keys::<FuelBlocks>(Some(IterDirection::Reverse))
        .next()
        .transpose()
}

/// Never prune through the chain tip (needed for block production / parent lookup).
fn cap_prune_before_respecting_tip(
    prune_before: BlockHeight,
    tip: BlockHeight,
) -> BlockHeight {
    if prune_before > tip {
        tip
    } else {
        prune_before
    }
}

/// Exclusive upper bound from a height whose header is older than `cutoff`.
fn height_after_old_block(height: BlockHeight) -> BlockHeight {
    BlockHeight::from(u32::from(height).saturating_add(1))
}

fn merge_prune_before(
    a: Option<BlockHeight>,
    b: Option<BlockHeight>,
) -> Option<BlockHeight> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x > y { x } else { y }),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

/// Find exclusive upper bound of heights to prune from onchain headers.
///
/// Returns `Some(h)` meaning delete history for all heights `< h`.
/// Returns `None` when nothing is older than `cutoff` (or the chain is empty).
/// The tip block is always kept.
pub fn prune_before_height(
    on_chain: &Database<OnChain>,
    cutoff: Tai64,
) -> StorageResult<Option<BlockHeight>> {
    let mut prune_before: Option<BlockHeight> = None;
    let mut tip: Option<BlockHeight> = None;
    for result in on_chain.iter_all::<FuelBlocks>(Some(IterDirection::Forward)) {
        let (height, block) = result?;
        tip = Some(height);
        if block.header().time() < cutoff {
            prune_before = Some(height_after_old_block(height));
        } else {
            break;
        }
    }
    let Some(tip) = tip else {
        return Ok(None);
    };
    Ok(prune_before.map(|before| cap_prune_before_respecting_tip(before, tip)))
}

/// Offchain `Old*` can retain pre-regenesis history absent from onchain `FuelBlocks`.
///
/// Uses a **contiguous height prefix** scan (same shape as `prune_before_height`).
/// Stops at the first in-window block or a gap in heights — an isolated high
/// `OldFuelBlocks` row with an ancient timestamp must not widen onchain deletes.
pub fn prune_before_from_offchain_history(
    off_chain: &Database<OffChain>,
    cutoff: Tai64,
) -> StorageResult<Option<BlockHeight>> {
    let mut prune_before: Option<BlockHeight> = None;
    let mut expected: Option<BlockHeight> = None;
    for result in off_chain.iter_all::<OldFuelBlocks>(Some(IterDirection::Forward)) {
        let (height, block) = result?;
        if let Some(next) = expected
            && height != next
        {
            break;
        }
        if block.header().time() < cutoff {
            prune_before = Some(height_after_old_block(height));
            expected = Some(height_after_old_block(height));
        } else {
            break;
        }
    }

    Ok(prune_before)
}

/// Resume partial prune: owned-tx index rows pointing at pruned onchain blocks.
pub fn prune_before_from_owned_index_gaps(
    on_chain: &Database<OnChain>,
    off_chain: &Database<OffChain>,
    tip: BlockHeight,
) -> StorageResult<Option<BlockHeight>> {
    let mut max_orphan_height: Option<BlockHeight> = None;
    for result in
        off_chain.iter_all_keys::<OwnedTransactions>(Some(IterDirection::Forward))
    {
        let key = result?;
        if key.block_height >= tip {
            continue;
        }
        if on_chain
            .storage::<FuelBlocks>()
            .get(&key.block_height)?
            .is_none()
        {
            max_orphan_height = max_orphan_height.map_or(Some(key.block_height), |m| {
                Some(if key.block_height > m {
                    key.block_height
                } else {
                    m
                })
            });
        }
    }
    Ok(max_orphan_height.map(height_after_old_block))
}

/// Resolve exclusive prune bound from headers, offchain history, and resume gaps.
pub fn resolve_prune_before(
    on_chain: &Database<OnChain>,
    off_chain: &Database<OffChain>,
    cutoff: Tai64,
) -> StorageResult<Option<BlockHeight>> {
    let tip = on_chain_tip_height(on_chain)?;
    let mut prune_before = merge_prune_before(
        prune_before_height(on_chain, cutoff)?,
        prune_before_from_offchain_history(off_chain, cutoff)?,
    );
    if let Some(tip) = tip {
        if let Some(ref mut before) = prune_before {
            *before = cap_prune_before_respecting_tip(*before, tip);
        }
        prune_before = merge_prune_before(
            prune_before,
            prune_before_from_owned_index_gaps(on_chain, off_chain, tip)?,
        );
        if let Some(ref mut before) = prune_before {
            *before = cap_prune_before_respecting_tip(*before, tip);
        }
    }
    Ok(prune_before)
}

/// Delete offchain + aged onchain history for heights `< prune_before` (scope B).
///
/// Does not touch Coins / Contracts* / Messages / UTXO / ProcessedTransactions.
/// Metadata / latest indexed height is unchanged.
pub fn prune_history(
    on_chain: &mut Database<OnChain>,
    off_chain: &mut Database<OffChain>,
    prune_before: BlockHeight,
    cutoff: Tai64,
) -> StorageResult<HistoryPruneReport> {
    let mut report = HistoryPruneReport {
        prune_before: Some(prune_before),
        ..Default::default()
    };

    let heights: Vec<BlockHeight> = on_chain
        .iter_all_keys::<FuelBlocks>(Some(IterDirection::Forward))
        .take_while(|result| match result {
            Ok(height) => *height < prune_before,
            Err(_) => true,
        })
        .collect::<StorageResult<Vec<_>>>()?;

    for chunk in heights.chunks(DELETE_BATCH_HEIGHTS) {
        prune_height_batch(on_chain, off_chain, chunk, &mut report)?;
    }

    prune_offchain_history_by_cutoff(off_chain, cutoff, prune_before, &mut report)?;
    prune_stale_transaction_statuses(off_chain, cutoff, prune_before, &mut report)?;

    report.deleted_owned_transactions = report
        .deleted_owned_transactions
        .saturating_add(prune_owned_transactions_before(off_chain, prune_before)?);

    Ok(report)
}

/// Backward-compatible alias used by earlier scaffolding.
pub fn prune_offchain_history(
    on_chain: &mut Database<OnChain>,
    off_chain: &mut Database<OffChain>,
    prune_before: BlockHeight,
    cutoff: Tai64,
) -> StorageResult<HistoryPruneReport> {
    prune_history(on_chain, off_chain, prune_before, cutoff)
}

/// Deletes offchain `Old*` / indexes / statuses for blocks outside retention
/// that were not walked via onchain `FuelBlocks` (e.g. pre-regenesis history).
fn prune_offchain_history_by_cutoff(
    off_chain: &mut Database<OffChain>,
    cutoff: Tai64,
    prune_before: BlockHeight,
    report: &mut HistoryPruneReport,
) -> StorageResult<()> {
    let stale_heights: Vec<BlockHeight> = off_chain
        .iter_all::<OldFuelBlocks>(Some(IterDirection::Forward))
        .filter_map(|result| match result {
            Ok((height, block)) => {
                if height < prune_before && block.header().time() < cutoff {
                    Some(Ok(height))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .collect::<StorageResult<Vec<_>>>()?;

    for chunk in stale_heights.chunks(DELETE_BATCH_HEIGHTS) {
        prune_offchain_height_batch(off_chain, chunk, report)?;
    }

    Ok(())
}

fn transaction_status_time(
    status: &fuel_core_types::services::transaction_status::TransactionExecutionStatus,
) -> Option<Tai64> {
    use fuel_core_types::services::transaction_status::TransactionExecutionStatus;
    match status {
        TransactionExecutionStatus::Success { time, .. }
        | TransactionExecutionStatus::Failed { time, .. }
        | TransactionExecutionStatus::Submitted { time, .. } => Some(*time),
        TransactionExecutionStatus::SqueezedOut { .. } => None,
    }
}

fn should_prune_transaction_status(
    status: &fuel_core_types::services::transaction_status::TransactionExecutionStatus,
    cutoff: Tai64,
    prune_before: BlockHeight,
) -> bool {
    use fuel_core_types::services::transaction_status::TransactionExecutionStatus;
    let Some(time) = transaction_status_time(status) else {
        return false;
    };
    if time >= cutoff {
        return false;
    }
    match status {
        TransactionExecutionStatus::Success { block_height, .. }
        | TransactionExecutionStatus::Failed { block_height, .. } => {
            *block_height < prune_before
        }
        TransactionExecutionStatus::Submitted { .. } => true,
        TransactionExecutionStatus::SqueezedOut { .. } => false,
    }
}

/// Delete aged statuses without `OldFuelBlocks` (`Submitted`, pre-regenesis leftovers).
/// Never removes statuses for blocks still retained at the tip.
fn prune_stale_transaction_statuses(
    off_chain: &mut Database<OffChain>,
    cutoff: Tai64,
    prune_before: BlockHeight,
    report: &mut HistoryPruneReport,
) -> StorageResult<()> {
    let tx_ids: Vec<_> = off_chain
        .iter_all::<TransactionStatuses>(Some(IterDirection::Forward))
        .filter_map(|result| match result {
            Ok((tx_id, status)) => {
                if should_prune_transaction_status(&status, cutoff, prune_before) {
                    Some(Ok(tx_id))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .collect::<StorageResult<Vec<_>>>()?;

    for chunk in tx_ids.chunks(DELETE_BATCH_HEIGHTS.saturating_mul(16)) {
        let mut transaction = StorageTransaction::transaction(
            off_chain.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        for tx_id in chunk {
            if transaction
                .storage_as_mut::<TransactionStatuses>()
                .take(tx_id)?
                .is_some()
            {
                report.deleted_transaction_statuses =
                    report.deleted_transaction_statuses.saturating_add(1);
            }
        }
        off_chain
            .data
            .commit_changes(None, StorageChanges::Changes(transaction.into_changes()))?;
    }
    Ok(())
}

fn prune_offchain_height_batch(
    off_chain: &mut Database<OffChain>,
    heights: &[BlockHeight],
    report: &mut HistoryPruneReport,
) -> StorageResult<()> {
    let mut block_snapshots = Vec::with_capacity(heights.len());
    for height in heights {
        let Some(block) = off_chain.storage::<OldFuelBlocks>().get(height)? else {
            continue;
        };
        block_snapshots.push((*height, block.into_owned()));
    }

    let mut transaction = StorageTransaction::transaction(
        off_chain.as_ref(),
        ConflictPolicy::Overwrite,
        Default::default(),
    );

    for (height, block) in &block_snapshots {
        let block_id = block.id();

        for tx_id in block.transactions() {
            if transaction
                .storage_as_mut::<TransactionStatuses>()
                .take(tx_id)?
                .is_some()
            {
                report.deleted_transaction_statuses =
                    report.deleted_transaction_statuses.saturating_add(1);
            }
            if transaction
                .storage_as_mut::<OldTransactions>()
                .take(tx_id)?
                .is_some()
            {
                report.deleted_old_transactions =
                    report.deleted_old_transactions.saturating_add(1);
            }
        }

        if transaction
            .storage_as_mut::<FuelBlockIdsToHeights>()
            .take(&block_id)?
            .is_some()
        {
            report.deleted_block_id_index =
                report.deleted_block_id_index.saturating_add(1);
        }

        if transaction
            .storage_as_mut::<OldFuelBlocks>()
            .take(height)?
            .is_some()
        {
            report.deleted_old_blocks = report.deleted_old_blocks.saturating_add(1);
        }

        if transaction
            .storage_as_mut::<OldFuelBlockConsensus>()
            .take(height)?
            .is_some()
        {
            report.deleted_old_consensus = report.deleted_old_consensus.saturating_add(1);
        }
    }

    off_chain
        .data
        .commit_changes(None, StorageChanges::Changes(transaction.into_changes()))?;

    Ok(())
}

fn prune_height_batch(
    on_chain: &mut Database<OnChain>,
    off_chain: &mut Database<OffChain>,
    heights: &[BlockHeight],
    report: &mut HistoryPruneReport,
) -> StorageResult<()> {
    // Snapshot block bodies + tx ids before mutating onchain.
    let mut block_snapshots = Vec::with_capacity(heights.len());
    for height in heights {
        let Some(block) = on_chain.storage::<FuelBlocks>().get(height)? else {
            continue;
        };
        block_snapshots.push((*height, block.into_owned()));
    }

    // --- Offchain deletes ---
    {
        let mut transaction = StorageTransaction::transaction(
            off_chain.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );

        for (height, block) in &block_snapshots {
            let block_id = block.id();

            for tx_id in block.transactions() {
                if transaction
                    .storage_as_mut::<TransactionStatuses>()
                    .take(tx_id)?
                    .is_some()
                {
                    report.deleted_transaction_statuses =
                        report.deleted_transaction_statuses.saturating_add(1);
                }
                if transaction
                    .storage_as_mut::<OldTransactions>()
                    .take(tx_id)?
                    .is_some()
                {
                    report.deleted_old_transactions =
                        report.deleted_old_transactions.saturating_add(1);
                }
            }

            if transaction
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .take(&block_id)?
                .is_some()
            {
                report.deleted_block_id_index =
                    report.deleted_block_id_index.saturating_add(1);
            }

            if transaction
                .storage_as_mut::<OldFuelBlocks>()
                .take(height)?
                .is_some()
            {
                report.deleted_old_blocks = report.deleted_old_blocks.saturating_add(1);
            }

            if transaction
                .storage_as_mut::<OldFuelBlockConsensus>()
                .take(height)?
                .is_some()
            {
                report.deleted_old_consensus =
                    report.deleted_old_consensus.saturating_add(1);
            }
        }

        // Bypass Modifiable height-linking: historical deletes must not advance metadata.
        off_chain
            .data
            .commit_changes(None, StorageChanges::Changes(transaction.into_changes()))?;
    }

    // --- Onchain deletes (scope B) ---
    {
        let mut transaction = StorageTransaction::transaction(
            on_chain.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );

        for (height, block) in &block_snapshots {
            for tx_id in block.transactions() {
                if transaction
                    .storage_as_mut::<Transactions>()
                    .take(tx_id)?
                    .is_some()
                {
                    report.deleted_onchain_transactions =
                        report.deleted_onchain_transactions.saturating_add(1);
                }
            }

            if transaction
                .storage_as_mut::<SealedBlockConsensus>()
                .take(height)?
                .is_some()
            {
                report.deleted_onchain_consensus =
                    report.deleted_onchain_consensus.saturating_add(1);
            }
        }

        let mut changes: Changes = transaction.into_changes();

        // FuelBlocks is Merklized: Storage remove is forbidden. Delete body bytes
        // via raw column ops; leave FuelBlockMerkleData + Latest metadata intact.
        let fuel_blocks_col = fuel_core_storage::column::Column::FuelBlocks.id();
        let block_entries = changes.entry(fuel_blocks_col).or_default();
        for (height, _) in &block_snapshots {
            let key = height.to_be_bytes().to_vec().into();
            block_entries.insert(key, WriteOperation::Remove);
            report.deleted_onchain_blocks =
                report.deleted_onchain_blocks.saturating_add(1);
        }

        on_chain
            .data
            .commit_changes(None, StorageChanges::Changes(changes))?;
    }

    Ok(())
}

fn prune_owned_transactions_before(
    off_chain: &mut Database<OffChain>,
    prune_before: BlockHeight,
) -> StorageResult<u64> {
    let keys: Vec<_> = off_chain
        .iter_all_keys::<OwnedTransactions>(Some(IterDirection::Forward))
        .filter_map(|result| match result {
            Ok(key) if key.block_height < prune_before => Some(Ok(key)),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<StorageResult<Vec<_>>>()?;

    let mut deleted = 0u64;
    for chunk in keys.chunks(DELETE_BATCH_HEIGHTS.saturating_mul(16)) {
        let mut transaction = StorageTransaction::transaction(
            off_chain.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        for key in chunk {
            if transaction
                .storage_as_mut::<OwnedTransactions>()
                .take(key)?
                .is_some()
            {
                deleted = deleted.saturating_add(1);
            }
        }
        off_chain
            .data
            .commit_changes(None, StorageChanges::Changes(transaction.into_changes()))?;
    }
    Ok(deleted)
}

/// GraphQL / API error message when status/receipts fall outside retention.
pub fn retention_window_error_message(retention: Duration) -> String {
    format!(
        "Transaction status outside history retention window ({})",
        format_retention_duration(retention)
    )
}

/// GraphQL / API error when a tx body was pruned by history retention.
pub fn retention_transaction_error_message(retention: Duration) -> String {
    format!(
        "Transaction outside history retention window ({})",
        format_retention_duration(retention)
    )
}

/// GraphQL / API error when a block was pruned by history retention.
pub fn retention_block_error_message(retention: Duration) -> String {
    format!(
        "Block outside history retention window ({})",
        format_retention_duration(retention)
    )
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        StorageAsMut,
        StorageAsRef,
        tables::Coins,
        transactional::WriteTransaction,
    };
    use fuel_core_types::{
        blockchain::{
            block::CompressedBlock,
            header::BlockHeader,
        },
        entities::coins::coin::CompressedCoin,
        fuel_tx::{
            Bytes32,
            UtxoId,
        },
        services::transaction_status::TransactionExecutionStatus,
    };
    use std::sync::Arc;

    fn block_at(height: u32, time: Tai64, tx_ids: Vec<Bytes32>) -> CompressedBlock {
        let mut block = CompressedBlock::default();
        *block.header_mut() = BlockHeader::new_block(height.into(), time);
        *block.transactions_mut() = tx_ids;
        block
    }

    #[test]
    fn format_retention_duration__prefers_days() {
        assert_eq!(
            format_retention_duration(Duration::from_secs(30 * 86_400)),
            "30d"
        );
        assert_eq!(
            format_retention_duration(Duration::from_secs(2 * 3_600)),
            "2h"
        );
        assert_eq!(format_retention_duration(Duration::from_secs(90)), "90s");
    }

    #[test]
    fn retention_window_error_message__names_retention() {
        let msg = retention_window_error_message(Duration::from_secs(30 * 86_400));
        assert!(msg.contains("history retention window"));
        assert!(msg.contains("30d"));
    }

    #[test]
    fn prune_before_height__finds_exclusive_bound() {
        let mut on_chain = Database::<OnChain>::in_memory();
        let t0 = Tai64::UNIX_EPOCH;
        let t_old = Tai64(t0.0.saturating_add(1));
        let t_keep = Tai64(t0.0.saturating_add(1_000));

        let tx_old = Bytes32::from([1; 32]);
        let tx_keep = Bytes32::from([2; 32]);

        on_chain
            .storage_as_mut::<FuelBlocks>()
            .insert(&0u32.into(), &block_at(0, t_old, vec![tx_old]))
            .unwrap();
        on_chain
            .storage_as_mut::<FuelBlocks>()
            .insert(&1u32.into(), &block_at(1, t_keep, vec![tx_keep]))
            .unwrap();

        // Cutoff between the two block times → prune height 0 only → prune_before = 1
        let cutoff = Tai64(t0.0.saturating_add(500));
        let before = prune_before_height(&on_chain, cutoff).unwrap();
        assert_eq!(before, Some(1u32.into()));
    }

    #[test]
    fn prune_history__scope_b_deletes_onchain_keeps_coins() {
        let mut on_chain = Database::<OnChain>::in_memory();
        let mut off_chain = Database::<OffChain>::in_memory();

        let t0 = Tai64::UNIX_EPOCH;
        let t_old = Tai64(t0.0.saturating_add(1));
        let t_keep = Tai64(t0.0.saturating_add(10_000));
        let tx_old = Bytes32::from([9; 32]);
        let tx_keep = Bytes32::from([8; 32]);

        let old_block = block_at(0, t_old, vec![tx_old]);
        let keep_block = block_at(1, t_keep, vec![tx_keep]);
        let old_id = old_block.id();
        let keep_id = keep_block.id();
        let utxo = UtxoId::new(tx_keep, 0);

        // Modifiable DB requires each commit after genesis to advance height.
        // Bundle per-height writes into one transaction.
        {
            let mut txn = on_chain.write_transaction();
            txn.storage_as_mut::<FuelBlocks>()
                .insert(&0u32.into(), &old_block)
                .unwrap();
            txn.storage_as_mut::<Transactions>()
                .insert(&tx_old, &Default::default())
                .unwrap();
            txn.commit().unwrap();
        }
        {
            let mut txn = on_chain.write_transaction();
            txn.storage_as_mut::<FuelBlocks>()
                .insert(&1u32.into(), &keep_block)
                .unwrap();
            txn.storage_as_mut::<Transactions>()
                .insert(&tx_keep, &Default::default())
                .unwrap();
            // Live UTXO that must survive prune.
            txn.storage_as_mut::<Coins>()
                .insert(&utxo, &CompressedCoin::default())
                .unwrap();
            txn.commit().unwrap();
        }

        {
            let mut txn = off_chain.write_transaction();
            txn.storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&old_id, &0u32.into())
                .unwrap();
            txn.storage_as_mut::<TransactionStatuses>()
                .insert(
                    &tx_old,
                    &TransactionExecutionStatus::Success {
                        block_height: 0u32.into(),
                        time: t_old,
                        result: None,
                        receipts: Arc::new(vec![]),
                        total_gas: 0,
                        total_fee: 0,
                    },
                )
                .unwrap();
            txn.storage_as_mut::<OldFuelBlocks>()
                .insert(&0u32.into(), &old_block)
                .unwrap();
            txn.commit().unwrap();
        }
        {
            let mut txn = off_chain.write_transaction();
            txn.storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&keep_id, &1u32.into())
                .unwrap();
            txn.storage_as_mut::<TransactionStatuses>()
                .insert(
                    &tx_keep,
                    &TransactionExecutionStatus::Success {
                        block_height: 1u32.into(),
                        time: t_keep,
                        result: None,
                        receipts: Arc::new(vec![]),
                        total_gas: 0,
                        total_fee: 0,
                    },
                )
                .unwrap();
            txn.commit().unwrap();
        }

        let cutoff = Tai64(t0.0.saturating_add(500));
        let prune_before = prune_before_height(&on_chain, cutoff)
            .unwrap()
            .expect("expected heights to prune");
        let report =
            prune_history(&mut on_chain, &mut off_chain, prune_before, cutoff).unwrap();

        assert_eq!(report.deleted_transaction_statuses, 1);
        assert_eq!(report.deleted_block_id_index, 1);
        assert_eq!(report.deleted_old_blocks, 1);
        assert_eq!(report.deleted_onchain_transactions, 1);
        assert_eq!(report.deleted_onchain_blocks, 1);

        // Offchain old status gone; kept status remains.
        assert!(
            off_chain
                .storage::<TransactionStatuses>()
                .get(&tx_old)
                .unwrap()
                .is_none()
        );
        assert!(
            off_chain
                .storage::<TransactionStatuses>()
                .get(&tx_keep)
                .unwrap()
                .is_some()
        );

        // Onchain: aged block + tx gone; retained height stays.
        assert!(
            on_chain
                .storage::<FuelBlocks>()
                .get(&0u32.into())
                .unwrap()
                .is_none()
        );
        assert!(
            on_chain
                .storage::<FuelBlocks>()
                .get(&1u32.into())
                .unwrap()
                .is_some()
        );
        assert!(
            on_chain
                .storage::<Transactions>()
                .get(&tx_old)
                .unwrap()
                .is_none()
        );
        assert!(
            on_chain
                .storage::<Transactions>()
                .get(&tx_keep)
                .unwrap()
                .is_some()
        );

        // Live coin untouched.
        assert!(on_chain.storage::<Coins>().get(&utxo).unwrap().is_some());
    }

    #[test]
    fn prune_before_height__never_prunes_past_tip() {
        let mut on_chain = Database::<OnChain>::in_memory();
        let t0 = Tai64::UNIX_EPOCH;
        let t_old = Tai64(t0.0.saturating_add(1));

        let tx = Bytes32::from([3; 32]);
        let block = block_at(0, t_old, vec![tx]);
        on_chain
            .storage_as_mut::<FuelBlocks>()
            .insert(&0u32.into(), &block)
            .unwrap();

        let cutoff = Tai64(t0.0.saturating_add(10_000));
        let before = prune_before_height(&on_chain, cutoff).unwrap();
        assert_eq!(before, Some(0u32.into()));
        assert!(
            on_chain
                .storage::<FuelBlocks>()
                .get(&0u32.into())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn prune_offchain_old_blocks_by_cutoff__pre_regenesis() {
        let mut on_chain = Database::<OnChain>::in_memory();
        let mut off_chain = Database::<OffChain>::in_memory();

        let t0 = Tai64::UNIX_EPOCH;
        let t_old = Tai64(t0.0.saturating_add(1));
        let t_keep = Tai64(t0.0.saturating_add(10_000));
        let tx_old = Bytes32::from([4; 32]);
        let old_block = block_at(5, t_old, vec![tx_old]);

        {
            let mut txn = on_chain.write_transaction();
            txn.storage_as_mut::<FuelBlocks>()
                .insert(
                    &100u32.into(),
                    &block_at(100, t_keep, vec![Bytes32::from([5; 32])]),
                )
                .unwrap();
            txn.commit().unwrap();
        }

        {
            let mut txn = off_chain.write_transaction();
            txn.storage_as_mut::<OldFuelBlocks>()
                .insert(&5u32.into(), &old_block)
                .unwrap();
            txn.storage_as_mut::<TransactionStatuses>()
                .insert(
                    &tx_old,
                    &TransactionExecutionStatus::Success {
                        block_height: 5u32.into(),
                        time: t_old,
                        result: None,
                        receipts: Arc::new(vec![]),
                        total_gas: 0,
                        total_fee: 0,
                    },
                )
                .unwrap();
            txn.commit().unwrap();
        }

        let cutoff = Tai64(t0.0.saturating_add(500));
        let prune_before = resolve_prune_before(&on_chain, &off_chain, cutoff)
            .unwrap()
            .expect("old offchain row should set bound");
        let report =
            prune_history(&mut on_chain, &mut off_chain, prune_before, cutoff).unwrap();

        assert!(report.deleted_old_blocks >= 1);
        assert!(
            off_chain
                .storage::<TransactionStatuses>()
                .get(&tx_old)
                .unwrap()
                .is_none()
        );
        assert!(
            on_chain
                .storage::<FuelBlocks>()
                .get(&100u32.into())
                .unwrap()
                .is_some()
        );
    }
}
