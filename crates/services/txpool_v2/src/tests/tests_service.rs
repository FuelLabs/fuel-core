use fuel_core_services::Service as ServiceTrait;
use fuel_core_types::{
    fuel_tx::UniqueIdentifier,
    fuel_types::ChainId,
    services::txpool::TransactionStatus,
};
use std::{
    sync::Arc,
    time::Duration,
};
use tokio_stream::StreamExt;

use crate::{
    config::Config,
    tests::universe::TestPoolUniverse,
    tx_status_stream::TxStatusMessage,
};

#[tokio::test]
async fn test_start_stop() {
    let service = TestPoolUniverse::default().build_service(None, None);
    service.start_and_await().await.unwrap();

    // Double start will return false.
    assert!(service.start().is_err(), "double start should fail");

    let state = service.stop_and_await().await.unwrap();
    assert!(state.stopped());
}

#[tokio::test]
async fn test_find() {
    let mut universe = TestPoolUniverse::default();

    let tx1 = Arc::new(universe.build_script_transaction(None, None, 10));
    let tx2 = Arc::new(universe.build_script_transaction(None, None, 20));
    let tx3 = Arc::new(universe.build_script_transaction(None, None, 30));

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let ids = vec![tx1.id(&Default::default()), tx2.id(&Default::default())];
    service
        .shared
        .insert(vec![tx1.clone(), tx2.clone()], None)
        .await
        .unwrap();

    universe
        .waiting_txs_insertion(service.shared.new_tx_notification_subscribe(), ids)
        .await;

    // When
    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx3.id(&Default::default()),
        ])
        .await
        .unwrap();

    // Then
    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    assert!(out[0].is_some(), "Tx1 should be some:{out:?}");
    let id = out[0].as_ref().unwrap().id();
    assert_eq!(id, tx1.id(&Default::default()), "Found tx id match{out:?}");
    assert!(out[1].is_none(), "Tx3 should not be found:{out:?}");
    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_prune_transactions() {
    const TIMEOUT: u64 = 3;
    let mut universe = TestPoolUniverse::default().config(Config {
        ttl_check_interval: Duration::from_secs(1),
        max_txs_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    });

    // Given
    let tx1 = Arc::new(universe.build_script_transaction(None, None, 10));
    let tx2 = Arc::new(universe.build_script_transaction(None, None, 20));
    let tx3 = Arc::new(universe.build_script_transaction(None, None, 30));
    let ids = vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ];

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    service
        .shared
        .insert(vec![tx1.clone(), tx2.clone(), tx3.clone()], None)
        .await
        .unwrap();

    universe
        .waiting_txs_insertion(service.shared.new_tx_notification_subscribe(), ids)
        .await;

    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
            tx3.id(&Default::default()),
        ])
        .await
        .unwrap();

    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");
    assert!(out[2].is_some(), "Tx3 should exist");

    // When
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
            tx3.id(&Default::default()),
        ])
        .await
        .unwrap();

    // Then
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_none(), "Tx3 should be pruned");

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_prune_transactions_the_oldest() {
    const TIMEOUT: u64 = 5;
    let mut universe = TestPoolUniverse::default().config(Config {
        ttl_check_interval: Duration::from_secs(TIMEOUT),
        max_txs_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    });

    let tx1 = Arc::new(universe.build_script_transaction(None, None, 10));
    let tx2 = Arc::new(universe.build_script_transaction(None, None, 20));
    let tx3 = Arc::new(universe.build_script_transaction(None, None, 30));
    let tx4 = Arc::new(universe.build_script_transaction(None, None, 40));

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    // insert tx1 at time `0`
    service
        .shared
        .insert(vec![tx1.clone()], None)
        .await
        .unwrap();

    // sleep for `4` seconds
    tokio::time::sleep(Duration::from_secs(4)).await;
    // insert tx2 at time `4`
    service
        .shared
        .insert(vec![tx2.clone()], None)
        .await
        .unwrap();

    universe
        .waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx2.id(&Default::default())],
        )
        .await;
    // check that tx1 and tx2 are still there at time `4`
    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
        ])
        .await
        .unwrap();

    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");

    // sleep for another `4` seconds
    tokio::time::sleep(Duration::from_secs(4)).await;
    // insert tx3 at time `8`
    service
        .shared
        .insert(vec![tx3.clone()], None)
        .await
        .unwrap();

    // sleep for `3` seconds
    tokio::time::sleep(Duration::from_secs(3)).await;

    // insert tx4 at time `11`
    service
        .shared
        .insert(vec![tx4.clone()], None)
        .await
        .unwrap();

    universe
        .waiting_txs_insertion(
            service.shared.new_tx_notification_subscribe(),
            vec![tx4.id(&Default::default())],
        )
        .await;
    // time is now `11`, tx1 and tx2 should be pruned
    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
            tx3.id(&Default::default()),
            tx4.id(&ChainId::default()),
        ])
        .await
        .unwrap();

    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_some(), "Tx3 should exist");
    assert!(out[3].is_some(), "Tx4 should exist");

    // sleep for `5` seconds
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;

    // time is now `16`, tx3 should be pruned
    let out = service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
            tx3.id(&Default::default()),
            tx4.id(&ChainId::default()),
        ])
        .await
        .unwrap();

    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_none(), "Tx3 should be pruned");
    assert!(out[3].is_some(), "Tx4 should exist");

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn simple_insert_removal_subscription() {
    const TIMEOUT: u64 = 2;
    let mut universe = TestPoolUniverse::default().config(Config {
        ttl_check_interval: Duration::from_secs(1),
        max_txs_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    });

    let tx1 = Arc::new(universe.build_script_transaction(None, None, 10));
    let tx2 = Arc::new(universe.build_script_transaction(None, None, 20));

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    let mut new_tx_notification = service.shared.new_tx_notification_subscribe();
    let mut tx1_subscribe_updates = service
        .shared
        .tx_update_subscribe(tx1.cached_id().unwrap())
        .unwrap();
    let mut tx2_subscribe_updates = service
        .shared
        .tx_update_subscribe(tx2.cached_id().unwrap())
        .unwrap();

    service
        .shared
        .insert(vec![tx1.clone(), tx2.clone()], None)
        .await
        .unwrap();

    assert_eq!(
        new_tx_notification.recv().await,
        Ok(tx1.cached_id().unwrap()),
        "First added should be tx1"
    );
    let update = tx1_subscribe_updates.next().await.unwrap();
    assert!(
        matches!(
            update,
            TxStatusMessage::Status(TransactionStatus::Submitted { .. })
        ),
        "First message in tx1 stream should be Submitted"
    );
    assert_eq!(
        new_tx_notification.recv().await,
        Ok(tx2.cached_id().unwrap()),
        "Second added should be tx2"
    );
    let update = tx2_subscribe_updates.next().await.unwrap();
    assert!(
        matches!(
            update,
            TxStatusMessage::Status(TransactionStatus::Submitted { .. })
        ),
        "First message in tx2 stream should be Submitted"
    );

    // waiting for them to be removed
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;

    let update = tx1_subscribe_updates.next().await.unwrap();
    assert_eq!(
        update,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            reason: "Transaction is removed: Transaction expired because it exceeded the configured time to live `tx-pool-ttl`."
                .to_string()
        }),
        "Second message in tx1 stream should be squeezed out"
    );

    let update = tx2_subscribe_updates.next().await.unwrap();
    assert_eq!(
        update,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            reason: "Transaction is removed: Transaction expired because it exceeded the configured time to live `tx-pool-ttl`."
                .to_string()
        }),
        "Second message in tx2 stream should be squeezed out"
    );

    service.stop_and_await().await.unwrap();
}
