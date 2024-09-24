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

use crate::{
    config::Config,
    tx_status_stream::TxStatusMessage,
};

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

    let out = service.shared.insert(vec![tx1.clone(), tx2.clone()]).await;

    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");
    assert!(out[1].is_ok(), "Tx2 should be OK, got err:{out:?}");
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx3.id(&Default::default()),
    ]);
    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    assert!(out[0].is_some(), "Tx1 should be some:{out:?}");
    let id = out[0].as_ref().unwrap().id();
    assert_eq!(id, tx1.id(&Default::default()), "Found tx id match{out:?}");
    assert!(out[1].is_none(), "Tx3 should not be found:{out:?}");
    service.stop_and_await().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn test_prune_transactions() {
    const TIMEOUT: u64 = 10;

    let config = Config {
        max_txs_ttl: Duration::from_secs(TIMEOUT),
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
        .insert(vec![tx1.clone(), tx2.clone(), tx3.clone()])
        .await;

    // Check that we have all transactions after insertion.
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");
    assert!(out[1].is_ok(), "Tx2 should be OK, got err:{out:?}");
    assert!(out[2].is_ok(), "Tx3 should be OK, got err:{out:?}");

    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ]);
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");
    assert!(out[2].is_some(), "Tx3 should exist");

    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
    ]);
    assert_eq!(out.len(), 3, "Should be len 3:{out:?}");
    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_none(), "Tx3 should be pruned");

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn test_prune_transactions_the_oldest() {
    const TIMEOUT: u64 = 5;

    let config = Config {
        max_txs_ttl: Duration::from_secs(TIMEOUT),
        ..Default::default()
    };
    let ctx = TestContextBuilder::new()
        .with_config(config)
        .build_and_start()
        .await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let tx3 = Arc::new(ctx.setup_script_tx(30));
    let tx4 = Arc::new(ctx.setup_script_tx(40));

    let service = ctx.service();

    // insert tx1 at time `0`
    let out = service.shared.insert(vec![tx1.clone()]).await;
    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{out:?}");

    // sleep for `4` seconds
    tokio::time::sleep(Duration::from_secs(4)).await;
    // insert tx2 at time `4`
    let out = service.shared.insert(vec![tx2.clone()]).await;
    assert!(out[0].is_ok(), "Tx2 should be OK, got err:{out:?}");

    // check that tx1 and tx2 are still there at time `4`
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
    ]);
    assert!(out[0].is_some(), "Tx1 should exist");
    assert!(out[1].is_some(), "Tx2 should exist");

    // sleep for another `4` seconds
    tokio::time::sleep(Duration::from_secs(4)).await;
    // insert tx3 at time `8`
    let out = service.shared.insert(vec![tx3.clone()]).await;
    assert!(out[0].is_ok(), "Tx3 should be OK, got err:{out:?}");

    // sleep for `3` seconds
    tokio::time::sleep(Duration::from_secs(3)).await;

    // insert tx4 at time `11`
    let out = service.shared.insert(vec![tx4.clone()]).await;
    assert!(out[0].is_ok(), "Tx4 should be OK, got err:{out:?}");

    // time is now `11`, tx1 and tx2 should be pruned
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
        tx4.id(&ChainId::default()),
    ]);
    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_some(), "Tx3 should exist");
    assert!(out[3].is_some(), "Tx4 should exist");

    // sleep for `5` seconds
    tokio::time::sleep(Duration::from_secs(TIMEOUT)).await;

    // time is now `16`, tx3 should be pruned
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
        tx3.id(&Default::default()),
        tx4.id(&ChainId::default()),
    ]);
    assert!(out[0].is_none(), "Tx1 should be pruned");
    assert!(out[1].is_none(), "Tx2 should be pruned");
    assert!(out[2].is_none(), "Tx3 should be pruned");
    assert!(out[3].is_some(), "Tx4 should exist");

    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn simple_insert_removal_subscription() {
    let ctx = TestContextBuilder::new().build_and_start().await;

    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let tx2 = Arc::new(ctx.setup_script_tx(20));
    let service = ctx.service();

    let mut new_tx_notification = service.shared.new_tx_notification_subscribe();
    let mut tx1_subscribe_updates = service
        .shared
        .tx_update_subscribe(tx1.cached_id().unwrap())
        .unwrap();
    let mut tx2_subscribe_updates = service
        .shared
        .tx_update_subscribe(tx2.cached_id().unwrap())
        .unwrap();

    let out = service.shared.insert(vec![tx1.clone(), tx2.clone()]).await;

    match &out[0] {
        Ok(_) => {
            assert_eq!(
                new_tx_notification.try_recv(),
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
        }
        Err(err) => {
            panic!("Tx1 should be OK, got err, {:?}", err)
        }
    }

    if out[1].is_ok() {
        assert_eq!(
            new_tx_notification.try_recv(),
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
    } else {
        panic!("Tx2 should be OK, got err");
    }

    // remove them
    service.shared.remove(vec![
        (
            tx1.cached_id().unwrap(),
            "Because of the test purposes".to_string(),
        ),
        (
            tx2.cached_id().unwrap(),
            "Because of the test purposes".to_string(),
        ),
    ]);

    let update = tx1_subscribe_updates.next().await.unwrap();
    assert_eq!(
        update,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            reason: "Transaction squeezed out because Because of the test purposes"
                .to_string()
        }),
        "Second message in tx1 stream should be squeezed out"
    );

    let update = tx2_subscribe_updates.next().await.unwrap();
    assert_eq!(
        update,
        TxStatusMessage::Status(TransactionStatus::SqueezedOut {
            reason: "Transaction squeezed out because Because of the test purposes"
                .to_string()
        }),
        "Second message in tx2 stream should be squeezed out"
    );

    service.stop_and_await().await.unwrap();
}
