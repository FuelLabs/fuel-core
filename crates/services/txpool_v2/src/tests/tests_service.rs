use fuel_core_services::Service as ServiceTrait;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Sealed,
    },
    fuel_tx::{
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::ChainId,
    services::{
        block_importer::ImportResult,
        txpool::TransactionStatus,
    },
};
use std::{
    sync::Arc,
    time::Duration,
};

use crate::{
    config::Config,
    tests::{
        mocks::MockImporter,
        universe::{
            TestPoolUniverse,
            DEFAULT_EXPIRATION_HEIGHT,
        },
    },
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

    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    let tx3 = universe.build_script_transaction(None, None, 30);

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    let ids = vec![tx1.id(&Default::default()), tx2.id(&Default::default())];
    service
        .shared
        .try_insert(vec![tx1.clone(), tx2.clone()])
        .unwrap();

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

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
    let id = out[0].as_ref().unwrap().tx().id();
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
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    let tx3 = universe.build_script_transaction(None, None, 30);
    let ids = vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ];

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    service
        .shared
        .try_insert(vec![tx1.clone(), tx2.clone(), tx3.clone()])
        .unwrap();

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

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

    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    let tx3 = universe.build_script_transaction(None, None, 30);
    let tx4 = universe.build_script_transaction(None, None, 40);

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    // Given
    // insert tx1 at time `0`
    service.shared.try_insert(vec![tx1.clone()]).unwrap();

    // sleep for `4` seconds
    tokio::time::sleep(Duration::from_secs(4)).await;
    // insert tx2 at time `4`
    service.shared.try_insert(vec![tx2.clone()]).unwrap();

    universe
        .await_expected_tx_statuses(
            vec![tx1.id(&Default::default()), tx2.id(&Default::default())],
            |status| matches!(status, TransactionStatus::Submitted { .. }),
        )
        .await
        .unwrap();

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
    service.shared.try_insert(vec![tx3.clone()]).unwrap();

    // sleep for `3` seconds
    tokio::time::sleep(Duration::from_secs(3)).await;

    // insert tx4 at time `11`
    service.shared.try_insert(vec![tx4.clone()]).unwrap();

    universe
        .await_expected_tx_statuses(
            vec![tx3.id(&Default::default()), tx4.id(&Default::default())],
            |status| matches!(status, TransactionStatus::Submitted { .. }),
        )
        .await
        .unwrap();

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
async fn prune_expired_transactions() {
    let mut universe = TestPoolUniverse::default();
    let (sender, receiver) = tokio::sync::mpsc::channel(10);

    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    let tx3 = universe.build_script_transaction(None, None, 30);

    let service =
        universe.build_service(None, Some(MockImporter::with_block_provider(receiver)));
    service.start_and_await().await.unwrap();

    // Given
    let expiration_block = Sealed {
        entity: {
            let mut block = Block::default();
            let header = block.header_mut();
            header.set_block_height(DEFAULT_EXPIRATION_HEIGHT);
            block
        },
        consensus: Default::default(),
    };
    let ids = vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ];
    service
        .shared
        .try_insert(vec![tx1.clone(), tx2.clone(), tx3.clone()])
        .unwrap();

    universe
        .await_expected_tx_statuses(ids.clone(), |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

    assert_eq!(
        service
            .shared
            .find(vec![
                tx1.id(&Default::default()),
                tx2.id(&Default::default()),
                tx3.id(&Default::default()),
            ])
            .await
            .unwrap()
            .iter()
            .filter(|x| x.is_some())
            .count(),
        3
    );

    // When
    sender
        .send(Arc::new(ImportResult::new_from_local(
            expiration_block,
            vec![],
            vec![],
        )))
        .await
        .unwrap();

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut { .. })
        })
        .await
        .unwrap();

    // Then
    assert!(service
        .shared
        .find(vec![
            tx1.id(&Default::default()),
            tx2.id(&Default::default()),
            tx3.id(&Default::default()),
        ])
        .await
        .unwrap()
        .iter()
        .all(|x| x.is_none()));

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn prune_expired_doesnt_trigger_twice() {
    let mut universe = TestPoolUniverse::default();
    let (sender, receiver) = tokio::sync::mpsc::channel(10);

    let (output_1, input_1) = universe.create_output_and_input();
    let (output_2, input_2) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output_1]), 10);
    let tx2 = universe.build_script_transaction(None, Some(vec![output_2]), 20);

    let service =
        universe.build_service(None, Some(MockImporter::with_block_provider(receiver)));
    service.start_and_await().await.unwrap();

    let ids = vec![tx1.id(&Default::default()), tx2.id(&Default::default())];
    service
        .shared
        .try_insert(vec![tx1.clone(), tx2.clone()])
        .unwrap();

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

    let tx3 = universe.build_script_transaction(
        Some(vec![
            input_1.into_input(UtxoId::new(tx1.id(&ChainId::default()), 0)),
            input_2.into_input(UtxoId::new(tx2.id(&ChainId::default()), 0)),
        ]),
        None,
        30,
    );

    let ids = vec![tx3.id(&Default::default())];
    service.shared.try_insert(vec![tx3.clone()]).unwrap();

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

    // Given
    let expiration_block = Sealed {
        entity: {
            let mut block = Block::default();
            let header = block.header_mut();
            header.set_block_height(DEFAULT_EXPIRATION_HEIGHT);
            block
        },
        consensus: Default::default(),
    };

    assert_eq!(
        service
            .shared
            .find(vec![
                tx1.id(&Default::default()),
                tx2.id(&Default::default()),
                tx3.id(&Default::default()),
            ])
            .await
            .unwrap()
            .iter()
            .filter(|x| x.is_some())
            .count(),
        3
    );

    // When
    sender
        .send(Arc::new(ImportResult::new_from_local(
            expiration_block,
            vec![],
            vec![],
        )))
        .await
        .unwrap();

    let ids = vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ];

    // Then
    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut { .. })
        })
        .await
        .unwrap();

    // Verify that their no new notifications about tx3
    let ids = vec![tx3.id(&Default::default())];
    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut { .. })
        })
        .await
        .unwrap_err()
        .is_timeout();

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn simple_insert_removal() {
    const TIMEOUT: u64 = 2;
    let mut universe = TestPoolUniverse::default().config(Config {
        ttl_check_interval: Duration::from_secs(1),
        max_txs_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    });

    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);

    let service = universe.build_service(None, None);
    service.start_and_await().await.unwrap();

    let ids = vec![tx1.id(&Default::default()), tx2.id(&Default::default())];
    service
        .shared
        .try_insert(vec![tx1.clone(), tx2.clone()])
        .unwrap();

    universe
        .await_expected_tx_statuses(ids.clone(), |status| {
            matches!(status, TransactionStatus::Submitted { .. })
        })
        .await
        .unwrap();

    // waiting for them to be removed
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;

    universe
        .await_expected_tx_statuses(ids, |status| {
            matches!(status, TransactionStatus::SqueezedOut { .. })
        })
        .await
        .unwrap();

    service.stop_and_await().await.unwrap();
}
