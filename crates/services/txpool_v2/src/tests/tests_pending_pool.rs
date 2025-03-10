use fuel_core_services::Service;
use fuel_core_types::{
    fuel_tx::{
        UniqueIdentifier,
        UtxoId,
    },
    services::txpool::TransactionStatus,
};

use crate::tests::universe::TestPoolUniverse;

#[tokio::test]
async fn test_tx__keep_missing_input_and_resolved_when_input_submitted() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_secs(1);
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
        .await_expected_tx_statuses(ids, |status| {
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

#[tokio::test]
async fn test_tx__return_error_expired() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_millis(100);
    universe.config.pending_pool_tx_ttl = timeout;

    let (_, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx1_id = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let missing_utxoid = *input.clone().utxo_id().unwrap();
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // When
    let ids = vec![tx2_id];
    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap_err()
        .is_timeout();
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

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
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut(s)
                if s.reason == squeezed_out_reason)
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
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut(s)
                if s.reason == squeezed_out_reason)
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}
