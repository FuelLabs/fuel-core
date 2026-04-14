//! Tests for preconfirmation rollback (issue #3098).
//!
//! When a block producer emits preconfirmations and then crashes or produces a
//! different block at the same height, the sentry/RPC mempool must purge the
//! stale preconfirmation state on the next canonical block import.

use std::sync::Arc;

use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Sealed,
    },
    fuel_tx::{
        Output,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::ImportResult,
        executor::{
            TransactionExecutionResult,
            TransactionExecutionStatus,
        },
        transaction_status::{
            PreConfirmationStatus,
            statuses,
        },
    },
};

use fuel_core_services::Service as ServiceTrait;

use crate::tests::{
    mocks::MockImporter,
    universe::TestPoolUniverse,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a canonical block sealed at `height` that contains `tx_ids`.
fn make_block_import(
    height: u32,
    tx_ids: &[fuel_core_types::fuel_tx::TxId],
) -> Arc<
    dyn std::ops::Deref<Target = fuel_core_types::services::block_importer::ImportResult>
        + Send
        + Sync,
> {
    let sealed_block = Sealed {
        entity: {
            let mut block = Block::default();
            block
                .header_mut()
                .set_block_height(BlockHeight::new(height));
            block
        },
        consensus: Default::default(),
    };
    let tx_statuses = tx_ids
        .iter()
        .map(|id| TransactionExecutionStatus {
            id: *id,
            result: TransactionExecutionResult::Success {
                result: None,
                receipts: Arc::new(vec![]),
                total_gas: 0,
                total_fee: 0,
            },
        })
        .collect();
    Arc::new(ImportResult::new_from_local(sealed_block, tx_statuses, vec![]).wrap())
}

/// Build a `PreConfirmationStatus::Success` that carries one coin output.
fn make_preconf_success(
    tx_id: fuel_core_types::fuel_tx::TxId,
    block_height: u32,
    output: Output,
) -> PreConfirmationStatus {
    let utxo_id = UtxoId::new(tx_id, 0);
    PreConfirmationStatus::Success(
        statuses::PreConfirmationSuccess {
            tx_pointer: TxPointer::new(BlockHeight::new(block_height), 0),
            total_gas: 0,
            total_fee: 0,
            receipts: None,
            resolved_outputs: Some(vec![(utxo_id, output)]),
        }
        .into(),
    )
}

/// Build a `PreConfirmationStatus::Success` with no resolved outputs.
fn make_preconf_success_no_outputs(
    _tx_id: fuel_core_types::fuel_tx::TxId,
    block_height: u32,
) -> PreConfirmationStatus {
    PreConfirmationStatus::Success(
        statuses::PreConfirmationSuccess {
            tx_pointer: TxPointer::new(BlockHeight::new(block_height), 0),
            total_gas: 0,
            total_fee: 0,
            receipts: None,
            resolved_outputs: None,
        }
        .into(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// After a preconfirmation arrives for tx T at height H, and then a canonical
/// block at height H is imported *without* T, the tx should no longer be
/// marked as "spent" — i.e. it can be re-inserted into the pool.
#[tokio::test]
async fn preconfirmed_tx_can_be_reinserted_after_rollback() {
    // Given
    let (block_sender, block_receiver) = tokio::sync::mpsc::channel(10);
    let mut universe = TestPoolUniverse::default();
    let tx = universe.build_script_transaction(None, None, 10);
    let tx_id = tx.id(&Default::default());

    let service = universe.build_service(
        None,
        Some(MockImporter::with_block_provider(block_receiver)),
    );
    service.start_and_await().await.unwrap();

    // Insert and wait for submitted status.
    service.shared.insert(tx.clone()).await.unwrap();
    universe
        .await_expected_tx_statuses_submitted(vec![tx_id])
        .await;

    // Simulate the block producer preconfirming tx at block height 1.
    universe.send_preconfirmation(tx_id, make_preconf_success_no_outputs(tx_id, 1));

    // Give the pool worker time to process the preconfirmation.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // The tx should not be in the pool any more (committed).
    let found = service.shared.find(vec![tx_id]).await.unwrap();
    assert!(
        found[0].is_none(),
        "tx should have been committed out of pool"
    );

    // When — import an empty block at height 1 (no tx T).
    block_sender.send(make_block_import(1, &[])).await.unwrap();

    // Give the worker time to process the block.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Then — re-inserting the same tx should now succeed because
    // spent_inputs was rolled back.
    service.shared.insert(tx.clone()).await.unwrap();
    universe
        .await_expected_tx_statuses_submitted(vec![tx_id])
        .await;

    service.stop_and_await().await.unwrap();
}

/// When a preconfirmed tx's outputs are used by a dependent tx D, and the
/// canonical block does not include the preconfirmed tx, D must be removed
/// from the pool.
#[tokio::test]
async fn dependents_of_preconfirmed_tx_removed_on_rollback() {
    // Given
    let (block_sender, block_receiver) = tokio::sync::mpsc::channel(10);
    let mut universe = TestPoolUniverse::default();

    // tx_parent is the tx that will be preconfirmed but not included.
    // It produces output_a (a coin).
    let (output_a, unset_input_a) = universe.create_output_and_input();
    let tx_parent = universe.build_script_transaction(None, Some(vec![output_a]), 1);
    let tx_parent_id = tx_parent.id(&Default::default());

    let service = universe.build_service(
        None,
        Some(MockImporter::with_block_provider(block_receiver)),
    );
    service.start_and_await().await.unwrap();

    // Simulate receiving a preconfirmation for tx_parent (which the sentry may
    // never have seen).  The preconf carries output_a.
    universe.send_preconfirmation(
        tx_parent_id,
        make_preconf_success(tx_parent_id, 1, output_a),
    );

    // Give the worker time to process the preconfirmation and register outputs.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Now insert tx_child that spends output_a (utxo from tx_parent).
    let input_a = unset_input_a.into_input(UtxoId::new(tx_parent_id, 0));
    let tx_child = universe.build_script_transaction(Some(vec![input_a]), None, 2);
    let tx_child_id = tx_child.id(&Default::default());

    service.shared.insert(tx_child.clone()).await.unwrap();
    universe
        .await_expected_tx_statuses_submitted(vec![tx_child_id])
        .await;

    // Sanity: child is in the pool.
    let found = service.shared.find(vec![tx_child_id]).await.unwrap();
    assert!(found[0].is_some(), "tx_child should be in pool");

    // When — import a block at height 1 that does NOT contain tx_parent.
    block_sender.send(make_block_import(1, &[])).await.unwrap();

    // Then — tx_child depends on a now-stale preconfirmed output; it must be
    // squeezed out.
    universe
        .await_expected_tx_statuses(vec![tx_child_id], |_, status| {
            matches!(
                status,
                fuel_core_types::services::transaction_status::TransactionStatus::SqueezedOut(_)
            )
        })
        .await
        .unwrap();

    let found = service.shared.find(vec![tx_child_id]).await.unwrap();
    assert!(found[0].is_none(), "tx_child should have been removed");

    service.stop_and_await().await.unwrap();
}

/// When a preconfirmed tx IS included in the canonical block, its state must
/// be committed normally — no spurious rollback.
#[tokio::test]
async fn preconfirmed_tx_committed_normally_when_in_canonical_block() {
    // Given
    let (block_sender, block_receiver) = tokio::sync::mpsc::channel(10);
    let mut universe = TestPoolUniverse::default();
    let tx = universe.build_script_transaction(None, None, 10);
    let tx_id = tx.id(&Default::default());

    let service = universe.build_service(
        None,
        Some(MockImporter::with_block_provider(block_receiver)),
    );
    service.start_and_await().await.unwrap();

    service.shared.insert(tx.clone()).await.unwrap();
    universe
        .await_expected_tx_statuses_submitted(vec![tx_id])
        .await;

    // Preconfirmation at height 1.
    universe.send_preconfirmation(tx_id, make_preconf_success_no_outputs(tx_id, 1));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // When — import a block at height 1 that CONTAINS the tx.
    block_sender
        .send(make_block_import(1, &[tx_id]))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Then — tx must not reappear in the pool; re-inserting it should fail
    // because its inputs are now permanently spent (committed in the block).
    let found = service.shared.find(vec![tx_id]).await.unwrap();
    assert!(
        found[0].is_none(),
        "tx should not be in pool after block commit"
    );

    service.stop_and_await().await.unwrap();
}

/// Stale preconfirmations at an older height are cleaned up when a later
/// block is imported, even if the heights don't match exactly.
#[tokio::test]
async fn stale_preconfs_at_older_height_cleaned_up_by_later_block() {
    // Given
    let (block_sender, block_receiver) = tokio::sync::mpsc::channel(10);
    let mut universe = TestPoolUniverse::default();

    let (output_a, unset_input_a) = universe.create_output_and_input();
    let tx_parent = universe.build_script_transaction(None, Some(vec![output_a]), 1);
    let tx_parent_id = tx_parent.id(&Default::default());

    let service = universe.build_service(
        None,
        Some(MockImporter::with_block_provider(block_receiver)),
    );
    service.start_and_await().await.unwrap();

    // Preconfirmation at height 1 (but the block producer crashes).
    universe.send_preconfirmation(
        tx_parent_id,
        make_preconf_success(tx_parent_id, 1, output_a),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Insert a dependent tx while the preconf outputs are "live".
    let input_a = unset_input_a.into_input(UtxoId::new(tx_parent_id, 0));
    let tx_child = universe.build_script_transaction(Some(vec![input_a]), None, 2);
    let tx_child_id = tx_child.id(&Default::default());

    service.shared.insert(tx_child).await.unwrap();
    universe
        .await_expected_tx_statuses_submitted(vec![tx_child_id])
        .await;

    // When — a block at height 2 arrives (skipping height 1). The preconf for
    // height 1 was never resolved, so it must be rolled back.
    block_sender.send(make_block_import(2, &[])).await.unwrap();

    // Then — tx_child (which depended on the stale preconf output) is removed.
    universe
        .await_expected_tx_statuses(vec![tx_child_id], |_, status| {
            matches!(
                status,
                fuel_core_types::services::transaction_status::TransactionStatus::SqueezedOut(_)
            )
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}
