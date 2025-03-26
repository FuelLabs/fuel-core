use fuel_core_services::Service;
use fuel_core_types::{
    fuel_tx::{
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::ChainId,
    services::transaction_status::TransactionStatus,
};

use crate::tests::universe::TestPoolUniverse;

#[tokio::test]
async fn insert_transaction__valid_tx_triggers_submitted() {
    let mut universe = TestPoolUniverse::default();
    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let tx1 = universe.build_script_transaction(None, None, 10);
    let ids = vec![tx1.id(&ChainId::default())];

    // When
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    // Then
    universe.await_expected_tx_statuses_submitted(ids).await;

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn insert_transaction__failed_tx_verification_triggers_squeezed_out() {
    let mut universe = TestPoolUniverse::default();
    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    // Mint transaction is not allowed
    let tx1 = universe.build_mint_transaction();

    // When
    service.shared.try_insert(vec![tx1.clone()]).unwrap();
    let ids = vec![tx1.id(&ChainId::default())];

    // Then
    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut(_))
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn insert_transaction__pool_notification_removed_triggers_squeezed_out() {
    let mut universe = TestPoolUniverse::default();
    universe.config.pool_limits.max_txs = 1;

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    const SMALL_TIP: u64 = 10;
    const LARGE_TIP: u64 = 1000;
    let tx1 = universe.build_script_transaction(None, None, SMALL_TIP);
    let tx1_id = tx1.id(&ChainId::default());
    let tx2 = universe.build_script_transaction(None, None, LARGE_TIP);

    // When
    service.shared.try_insert(vec![tx1.clone()]).unwrap();
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // Then
    // Since the pool capacity is 1, the transaction with small tip should be squeezed out
    // and replaced with a more lucrative transaction.
    universe
        .await_expected_tx_statuses(vec![tx1_id], |status| {
            matches!(status, TransactionStatus::SqueezedOut(_))
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn insert_transaction__pool_notification_error_insertion_triggers_squeezed_out() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_millis(100);
    universe.config.pending_pool_tx_ttl = timeout;
    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let (_, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx1_id = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    // Tx2 has an input that doesn't exist
    let tx2 = universe.build_script_transaction(Some(vec![input.clone()]), None, 20);
    let tx2_id = tx2.id(&Default::default());

    // When
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // Then
    universe
        .await_expected_tx_statuses(vec![tx2_id], |status| {
            matches!(status, TransactionStatus::SqueezedOut(_))
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}
