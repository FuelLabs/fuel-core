use fuel_core_services::Service;
use fuel_core_types::{
    fuel_tx::{
        UniqueIdentifier,
        UtxoId,
    },
    services::txpool::TransactionStatus,
};
use futures::StreamExt;

use crate::{
    tests::universe::TestPoolUniverse,
    TxStatusMessage,
};

#[tokio::test]
async fn test_tx__keep_missing_input_and_resolved_when_input_submitted() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    let timeout = tokio::time::Duration::from_secs(1);
    universe.config.pending_pool_tx_ttl = timeout;

    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx_id1 = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx_id1, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx_id2 = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let mut status_stream = service.shared.tx_update_subscribe(tx_id2).unwrap();
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // When
    tokio::time::timeout(timeout, status_stream.next())
        .await
        .unwrap_err();
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    universe
        .waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx1.id(&Default::default()), tx2.id(&Default::default())],
        )
        .await;

    // Then
    let status = status_stream.next().await.unwrap();
    assert!(matches!(
        status,
        TxStatusMessage::Status(TransactionStatus::Submitted { .. })
    ));

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
    let tx_id1 = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx_id1, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx_id2 = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    service.shared.try_insert(vec![tx2.clone()]).unwrap();
    let mut subscriber_status = service.shared.tx_update_subscribe(tx_id2).unwrap();

    // When
    tokio::time::timeout(
        timeout,
        universe.waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx2.id(&Default::default())],
        ),
    )
    .await
    .unwrap_err();
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    // Then
    let status = subscriber_status.next().await.unwrap();
    // The error returned is the error that the transaction was squeezed out for.
    // We don't need the user to know that pending pool exists
    assert_eq!(
        status,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            tx_id: tx_id2,
            reason: "Transaction input validation failed: UTXO \
        (id: cf6532b2371b3efb71ff8ca4ec32cf046fb1be64e620d21a0a98c0298f8196140000) \
        does not exist"
                .to_string(),
        })
    );

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_tx__directly_removed_not_enough_space() {
    let mut universe = TestPoolUniverse::default();
    universe.config.utxo_validation = true;
    universe.config.max_pending_pool_size_percentage = 1.into();
    universe.config.pool_limits.max_txs = 1;

    let (_, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx_id1 = tx1.id(&Default::default());
    let input = unset_input.into_input(UtxoId::new(tx_id1, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx_id2 = tx2.id(&Default::default());

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    // When
    let mut subscriber_status = service.shared.tx_update_subscribe(tx_id2).unwrap();
    let status = subscriber_status.next().await.unwrap();

    // Then
    // The error returned is the error that the transaction was squeezed out for.
    // We don't need the user to know that pending pool exists
    assert_eq!(
        status,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            tx_id: tx_id2,
            reason: "Transaction input validation failed: UTXO \
            (id: cf6532b2371b3efb71ff8ca4ec32cf046fb1be64e620d21a0a98c0298f8196140000) \
            does not exist"
                .to_string(),
        })
    );

    service.stop_and_await().await.unwrap();
}
