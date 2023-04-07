use super::*;
use crate::service::test_helpers::{
    TestContext,
    TestContextBuilder,
};
use fuel_core_services::Service as ServiceTrait;
use fuel_core_types::{
    fuel_tx::UniqueIdentifier,
    services::txpool::Error as TxpoolError,
};
use std::time::Duration;

#[tokio::test]
async fn test_start_stop() {
    let ctx = TestContext::new().await;

    let service = ctx.service();

    // Double start will return false.
    assert!(service.start().is_err(), "double start should fail");

    let state = service.stop_and_await().await.unwrap();
    assert!(state.stopped());
}

#[tokio::test]
async fn test_find() {
    let ctx = TestContext::new().await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let tx3 = Arc::new(ctx.setup_script_tx(30));

    let service = ctx.service();

    let out = service.shared.insert(vec![tx1.clone(), tx2.clone()]);

    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");
    assert!(out[1].is_ok(), "Tx2 should be OK, got err:{out:?}");
    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    assert!(out[0].is_some(), "Tx1 should be some:{out:?}");
    let id = out[0].as_ref().unwrap().id();
    assert_eq!(
        id,
        tx1.id(&ConsensusParameters::DEFAULT),
        "Found tx id match{out:?}"
    );
    assert!(out[1].is_none(), "Tx3 should not be found:{out:?}");
    service.stop_and_await().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn test_prune_transactions() {
    const TIMEOUT: u64 = 10;

    let config = Config {
        transaction_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    };
    let ctx = TestContextBuilder::new()
        .with_config(config)
        .build_and_start()
        .await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let tx3 = Arc::new(ctx.setup_script_tx(30));

    let service = ctx.service();

    let out = service
        .shared
        .insert(vec![tx1.clone(), tx2.clone(), tx3.clone()]);

    // Check that we have all transactions after insertion.
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");
    assert!(out[1].is_ok(), "Tx2 should be OK, got err:{out:?}");
    assert!(out[2].is_ok(), "Tx3 should be OK, got err:{out:?}");

    tokio::time::sleep(Duration::from_secs(TIMEOUT / 2)).await;
    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");
    assert!(out[2].is_some(), "Tx3 should exist");

    tokio::time::sleep(Duration::from_secs(TIMEOUT / 2 + 1)).await;
    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_none(), "Tx3 should be pruned");

    service.stop_and_await().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn test_prune_transactions_the_oldest() {
    const TIMEOUT: u64 = 10;
    const DELAY: u64 = 2;

    let config = Config {
        transaction_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    };
    let ctx = TestContextBuilder::new()
        .with_config(config)
        .build_and_start()
        .await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let tx3 = Arc::new(ctx.setup_script_tx(30));

    let service = ctx.service();

    let out = service.shared.insert(vec![tx1.clone()]);
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");

    tokio::time::sleep(Duration::from_secs(TIMEOUT - DELAY)).await;
    let out = service.shared.insert(vec![tx2.clone()]);
    assert!(out[0].is_ok(), "Tx2 should be OK, got err:{out:?}");

    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");

    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    let out = service.shared.insert(vec![tx3.clone()]);
    assert!(out[0].is_ok(), "Tx3 should be OK, got err:{out:?}");

    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert!(out[0].is_none(), "Tx1 should pruned");
    assert!(out[1].is_some(), "Tx2 should exist");
    assert!(out[2].is_some(), "Tx3 should exist");

    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;

    let out = service.shared.find(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
        tx3.id(&ConsensusParameters::DEFAULT),
    ]);
    assert!(out[0].is_none(), "Tx1 should pruned");
    assert!(out[1].is_none(), "Tx2 should pruned");
    assert!(out[2].is_some(), "Tx3 should exist");

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn simple_insert_removal_subscription() {
    let ctx = TestContextBuilder::new().build_and_start().await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let service = ctx.service();

    let mut subscribe_status = service.shared.tx_status_subscribe();
    let mut subscribe_update = service.shared.tx_update_subscribe();

    let out = service.shared.insert(vec![tx1.clone(), tx2.clone()]);

    if let Ok(tx) = &out[0] {
        assert_eq!(
            subscribe_status.try_recv(),
            Ok(TxStatus::Submitted),
            "First added should be tx1"
        );
        let update = subscribe_update.try_recv().unwrap();
        assert_eq!(
            *update.tx_id(),
            tx.inserted.id(),
            "First added should be tx1"
        );
    } else {
        panic!("Tx1 should be OK, got err");
    }

    if let Ok(tx) = &out[1] {
        assert_eq!(
            subscribe_status.try_recv(),
            Ok(TxStatus::Submitted),
            "Second added should be tx2"
        );
        let update = subscribe_update.try_recv().unwrap();
        assert_eq!(
            *update.tx_id(),
            tx.inserted.id(),
            "Second added should be tx2"
        );
    } else {
        panic!("Tx2 should be OK, got err");
    }

    // remove them
    let rem = service.shared.remove(vec![
        tx1.id(&ConsensusParameters::DEFAULT),
        tx2.id(&ConsensusParameters::DEFAULT),
    ]);

    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), subscribe_status.recv())
            .await,
        Ok(Ok(TxStatus::SqueezedOut {
            reason: TxpoolError::Removed
        })),
        "First removed should be tx1"
    );
    let update =
        tokio::time::timeout(std::time::Duration::from_secs(2), subscribe_update.recv())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(*update.tx_id(), rem[0].id(), "First removed should be tx1");

    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), subscribe_status.recv())
            .await,
        Ok(Ok(TxStatus::SqueezedOut {
            reason: TxpoolError::Removed
        })),
        "Second removed should be tx2"
    );
    let update =
        tokio::time::timeout(std::time::Duration::from_secs(2), subscribe_update.recv())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(*update.tx_id(), rem[1].id(), "Second removed should be tx2");

    service.stop_and_await().await.unwrap();
}
