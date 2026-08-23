use std::sync::Arc;

use fuel_core_services::Service;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Sealed,
    },
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        AssetId,
        Input,
        Output,
        UniqueIdentifier,
        UtxoId,
    },
    services::{
        block_importer::ImportResult,
        executor::{
            TransactionExecutionResult,
            TransactionExecutionStatus,
        },
        transaction_status::TransactionStatus,
    },
};

use crate::tests::{
    mocks::MockImporter,
    universe::{
        IntoEstimated,
        TestPoolUniverse,
    },
};

#[tokio::test]
async fn test_tx__keep_missing_input_and_resolved_when_input_submitted() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_millis(10000);
    universe.config.pending_pool_tx_ttl = timeout;

    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx1_id = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let ids = vec![tx2_id];
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    universe
        .await_expected_tx_statuses(ids, |_, status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap_err()
        .is_timeout();

    // When
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    // Then
    let ids = vec![tx1_id, tx2_id];
    universe.await_expected_tx_statuses_submitted(ids).await;

    service.stop_and_await().await.unwrap();
}

/// End-to-end regression for the "lost dependent transaction" stall observed
/// with o2 deploy chains: a child spending the CHANGE output of a
/// not-yet-committed parent lands in the pending pool (change amounts are
/// unknown pre-execution). When the parent's block commits, the worker's
/// `process_block` resolves pending transactions from the block's executed
/// outputs — which MUST include `Change`/`Variable`. Before the fix those were
/// skipped, so the child was never promoted and sat in the pending pool until
/// its TTL squeezed it out (minutes-long stalls of otherwise-ready chains).
#[tokio::test]
async fn test_tx__pending_on_change_output_resolves_when_parent_block_commits() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    universe.config.pending_pool_tx_ttl = tokio::time::Duration::from_millis(10000);

    // Given: parent tx1 with a change output at index 0, and child tx2 that
    // spends that change UTXO.
    let predicate_code: Vec<u8> =
        fuel_core_types::fuel_asm::op::ret(1).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate_code);
    let tx1 = universe.build_script_transaction(
        None,
        Some(vec![Output::change(owner, 0, AssetId::BASE)]),
        10,
    );
    let tx1_id = tx1.id(&Default::default());
    let change_utxo = UtxoId::new(tx1_id, 0);
    let child_input = universe
        .custom_predicate(AssetId::BASE, 100, predicate_code, Some(change_utxo))
        .into_default_estimated();
    let tx2 = universe.build_script_transaction(Some(vec![child_input]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    let (block_sender, block_receiver) = tokio::sync::mpsc::channel(10);
    let service = universe.build_service(
        None,
        Some(MockImporter::with_block_provider(block_receiver)),
    );
    service.start_and_await().await.unwrap();

    // The child arrives while the parent is not committed yet: it must wait in
    // the pending pool (no `Submitted` status).
    service.shared.try_insert(vec![tx2.clone()]).unwrap();
    universe
        .await_expected_tx_statuses(vec![tx2_id], |_, status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap_err()
        .is_timeout();

    // When: the parent's block commits. The importer writes the executed
    // change coin to the database and then broadcasts the import result.
    {
        let mut coin = CompressedCoin::default();
        coin.set_owner(owner);
        coin.set_amount(100);
        coin.set_asset_id(AssetId::BASE);
        universe
            .database_mut()
            .data
            .lock()
            .unwrap()
            .coins
            .insert(change_utxo, coin);
    }
    let block = Sealed {
        entity: {
            let mut block = Block::default();
            block.header_mut().set_block_height(1u32.into());
            block.transactions_mut().push(tx1.clone());
            block
        },
        consensus: Default::default(),
    };
    let tx_status = TransactionExecutionStatus {
        id: tx1_id,
        result: TransactionExecutionResult::Success {
            result: None,
            receipts: Arc::new(vec![]),
            total_gas: 0,
            total_fee: 0,
        },
    };
    block_sender
        .send(Arc::new(
            ImportResult::new_from_local(block, vec![tx_status], vec![]).wrap(),
        ))
        .await
        .unwrap();

    // Then: the child is resolved from the pending pool and inserted into the
    // main pool promptly (no TTL wait).
    universe
        .await_expected_tx_statuses_submitted(vec![tx2_id])
        .await;

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_tx__return_error_expired() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_millis(100);
    universe.config.pending_pool_tx_ttl = timeout;

    // Given
    let (_, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx1_id = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let missing_utxoid = *input.clone().utxo_id().unwrap();
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // When
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // Then
    // The error returned is the error that the transaction was squeezed out for.
    // We don't need the user to know that pending pool exists
    let ids = vec![tx2_id];
    let squeezed_out_reason = format!(
        "Transaction input validation failed: UTXO \
        (id: {missing_utxoid}) \
        does not exist"
    );
    universe
        .await_expected_tx_statuses(ids, |tx_id, status| {
            matches!(status, TransactionStatus::SqueezedOut(s)
                if s.reason() == format!("{squeezed_out_reason} TxId: {tx_id}"))
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_tx__directly_removed_not_enough_space() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    universe.config.max_pending_pool_size_percentage = 1;
    universe.config.pool_limits.max_txs = 1;

    let (_, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx_id1 = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx_id1, 0));
    let missing_utxoid = *input.clone().utxo_id().unwrap();
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    // When
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // Then
    // The error returned is the error that the transaction was squeezed out for.
    // We don't need the user to know that pending pool exists
    let ids = vec![tx2_id];
    let squeezed_out_reason = format!(
        "Transaction input validation failed: UTXO \
        (id: {missing_utxoid}) \
        does not exist"
    );
    universe
        .await_expected_tx_statuses(ids, |tx_id, status| {
            matches!(status, TransactionStatus::SqueezedOut(s)
                if s.reason() == format!("{} TxId: {}", squeezed_out_reason, tx_id))
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}
