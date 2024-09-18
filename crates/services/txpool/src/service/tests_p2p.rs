use super::*;
use crate::{
    service::test_helpers::{
        MockP2P,
        TestContextBuilder,
    },
    test_helpers::TEST_COIN_AMOUNT,
};
use fuel_core_services::Service;
use fuel_core_storage::rand::{
    prelude::StdRng,
    SeedableRng,
};
use fuel_core_types::{
    fuel_tx::{
        field::Inputs,
        AssetId,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
};
use std::{
    ops::Deref,
    time::Duration,
};
use tokio::sync::Notify;

#[tokio::test]
async fn test_new_subscription_p2p() {
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);
    let tx2 = ctx_builder.setup_script_tx(100);

    // Given
    let wait_notification = Arc::new(Notify::new());
    let notifier = wait_notification.clone();
    let mut p2p = MockP2P::new();
    p2p.expect_gossiped_transaction_events()
        .returning(|| Box::pin(fuel_core_services::stream::pending()));
    p2p.expect_subscribe_new_peers().returning(|| {
        // Return a boxstream that yield one element
        Box::pin(fuel_core_services::stream::unfold(0, |state| async move {
            if state == 0 {
                Some((PeerId::from(vec![1, 2]), state + 1))
            } else {
                None
            }
        }))
    });
    let tx1_clone = tx1.clone();
    let tx2_clone = tx2.clone();
    p2p.expect_request_tx_ids().return_once(move |peer_id| {
        assert_eq!(peer_id, PeerId::from(vec![1, 2]));
        Ok(vec![
            tx1_clone.id(&ChainId::default()),
            tx2_clone.id(&ChainId::default()),
        ])
    });
    let tx1_clone = tx1.clone();
    let tx2_clone = tx2.clone();
    p2p.expect_request_txs()
        .return_once(move |peer_id, tx_ids| {
            assert_eq!(peer_id, PeerId::from(vec![1, 2]));
            assert_eq!(
                tx_ids,
                vec![
                    tx1_clone.id(&ChainId::default()),
                    tx2_clone.id(&ChainId::default())
                ]
            );
            notifier.notify_one();
            Ok(vec![Some(tx1_clone), Some(tx2_clone)])
        });
    ctx_builder.with_p2p(p2p);
    let ctx = ctx_builder.build();
    let service = ctx.service();

    service.start_and_await().await.unwrap();

    wait_notification.notified().await;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // When
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
    ]);

    // Then
    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    for (tx_pool, tx_expected) in out.into_iter().zip(&[tx1, tx2]) {
        assert!(tx_pool.is_some(), "Tx should be some:{tx_pool:?}");
        assert_eq!(
            tx_pool.unwrap().id(),
            tx_expected.id(&Default::default()),
            "Found tx id didn't match"
        );
    }
}

#[tokio::test]
async fn test_new_subscription_p2p_ask_subset_of_transactions() {
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);
    let tx2 = ctx_builder.setup_script_tx(100);

    // Given
    let wait_notification = Arc::new(Notify::new());
    let notifier = wait_notification.clone();
    let mut p2p = MockP2P::new();
    p2p.expect_gossiped_transaction_events()
        .returning(|| Box::pin(fuel_core_services::stream::pending()));
    p2p.expect_subscribe_new_peers().returning(|| {
        // Return a boxstream that yield one element
        Box::pin(fuel_core_services::stream::unfold(0, |state| async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            if state == 0 {
                Some((PeerId::from(vec![1, 2]), state + 1))
            } else {
                None
            }
        }))
    });
    let tx1_clone = tx1.clone();
    let tx2_clone = tx2.clone();
    p2p.expect_request_tx_ids().returning(move |peer_id| {
        assert_eq!(peer_id, PeerId::from(vec![1, 2]));
        Ok(vec![
            tx1_clone.id(&ChainId::default()),
            tx2_clone.id(&ChainId::default()),
        ])
    });
    let tx2_clone = tx2.clone();
    p2p.expect_request_txs()
        .return_once(move |peer_id, tx_ids| {
            assert_eq!(peer_id, PeerId::from(vec![1, 2]));
            assert_eq!(tx_ids, vec![tx2_clone.id(&ChainId::default())]);
            notifier.notify_one();
            Ok(vec![Some(tx2_clone)])
        });
    ctx_builder.with_p2p(p2p);
    let ctx = ctx_builder.build();
    let service = ctx.service();

    service.start_and_await().await.unwrap();
    service.shared.insert(vec![Arc::new(tx1.clone())]).await;

    wait_notification.notified().await;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let out = service.shared.find(vec![
        tx1.id(&Default::default()),
        tx2.id(&Default::default()),
    ]);
    assert_eq!(out.len(), 2, "Should be len 2:{out:?}");
    for (tx_pool, tx_expected) in out.into_iter().zip(&[tx1, tx2]) {
        assert!(tx_pool.is_some(), "Tx should be some:{tx_pool:?}");
        assert_eq!(
            tx_pool.unwrap().id(),
            tx_expected.id(&Default::default()),
            "Found tx id didn't match"
        );
    }
}

#[tokio::test]
async fn can_insert_from_p2p() {
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);

    let p2p = MockP2P::new_with_txs(vec![tx1.clone()]);
    ctx_builder.with_p2p(p2p);

    let ctx = ctx_builder.build();
    let service = ctx.service();
    let mut receiver = service
        .shared
        .tx_update_subscribe(tx1.id(&Default::default()))
        .unwrap();

    service.start_and_await().await.unwrap();

    let res = receiver.next().await;
    assert!(matches!(
        res,
        Some(TxStatusMessage::Status(TransactionStatus::Submitted { .. }))
    ));

    // fetch tx from pool
    let out = service.shared.find(vec![tx1.id(&Default::default())]);

    let got_tx: Transaction = out[0].as_ref().unwrap().tx().clone().deref().into();
    assert_eq!(tx1, got_tx);
}

#[tokio::test]
async fn insert_from_local_broadcasts_to_p2p() {
    // setup initial state
    let mut ctx_builder = TestContextBuilder::new();
    // add coin to builder db and generate a valid tx
    let tx1 = ctx_builder.setup_script_tx(10);

    let mut p2p = MockP2P::new_with_txs(vec![]);
    let mock_tx1 = tx1.clone();
    p2p.expect_broadcast_transaction()
        .withf(move |receive: &Arc<Transaction>| **receive == mock_tx1)
        .times(1)
        .returning(|_| Ok(()));

    ctx_builder.with_p2p(p2p);
    // build and start the txpool service
    let ctx = ctx_builder.build_and_start().await;

    let service = ctx.service();
    let mut new_tx_notification = service.shared.new_tx_notification_subscribe();
    let mut subscribe_update = service
        .shared
        .tx_update_subscribe(tx1.cached_id().unwrap())
        .unwrap();

    let out = service.shared.insert(vec![Arc::new(tx1.clone())]).await;

    if out[0].is_ok() {
        // we are sure that included tx are already broadcasted.

        // verify status updates
        assert_eq!(
            new_tx_notification.try_recv(),
            Ok(tx1.cached_id().unwrap()),
            "First added should be tx1"
        );
        let update = subscribe_update.next().await;
        assert!(
            matches!(
                update,
                Some(TxStatusMessage::Status(TransactionStatus::Submitted { .. }))
            ),
            "Got {:?}",
            update
        );
    } else {
        panic!("Tx1 should be OK, got err");
    }
}

#[tokio::test]
async fn test_insert_from_p2p_does_not_broadcast_to_p2p() {
    let mut ctx_builder = TestContextBuilder::new();
    // add coin to builder db and generate a valid tx
    let tx1 = ctx_builder.setup_script_tx(10);
    // setup p2p mock - with tx incoming from p2p
    let txs = vec![tx1.clone()];
    let mut p2p = MockP2P::new_with_txs(txs);
    let (send, mut receive) = broadcast::channel::<()>(1);
    p2p.expect_broadcast_transaction().returning(move |_| {
        send.send(()).unwrap();
        Ok(())
    });
    ctx_builder.with_p2p(p2p);

    // build and start the txpool service
    let ctx = ctx_builder.build();
    let service = ctx.service();
    // verify tx status update from p2p injected tx is successful
    let mut receiver = service
        .shared
        .tx_update_subscribe(tx1.id(&Default::default()))
        .unwrap();

    service.start_and_await().await.unwrap();

    let res = receiver.next().await;
    assert!(matches!(
        res,
        Some(TxStatusMessage::Status(TransactionStatus::Submitted { .. }))
    ));

    // verify tx was not broadcast to p2p
    let not_broadcast =
        tokio::time::timeout(Duration::from_millis(100), receive.recv()).await;
    assert!(
        not_broadcast.is_err(),
        "expected a timeout because no broadcast should have occurred"
    )
}

#[tokio::test]
async fn test_gossipped_transaction_with_check_error_rejected() {
    // verify that gossipped transactions which fail basic sanity checks are rejected (punished)

    let mut ctx_builder = TestContextBuilder::new();
    // add coin to builder db and generate a valid tx
    let mut tx1 = ctx_builder.setup_script_tx(10);
    // now intentionally muck up the tx such that it will return a CheckError,
    // by duplicating an input
    let script = tx1.as_script_mut().unwrap();
    let input = script.inputs()[0].clone();
    script.inputs_mut().push(input);
    // setup p2p mock - with tx incoming from p2p
    let txs = vec![tx1.clone()];
    let mut p2p = MockP2P::new_with_txs(txs);
    let (send, mut receive) = broadcast::channel::<()>(1);
    p2p.expect_notify_gossip_transaction_validity()
        .returning(move |_, validity| {
            // Expect the transaction to be rejected
            assert_eq!(validity, GossipsubMessageAcceptance::Reject);
            // Notify test that the gossipsub acceptance was set
            send.send(()).unwrap();
            Ok(())
        });
    ctx_builder.with_p2p(p2p);

    // build and start the txpool service
    let ctx = ctx_builder.build();
    let service = ctx.service();
    service.start_and_await().await.unwrap();
    // verify p2p was notified about the transaction validity
    let gossip_validity_notified =
        tokio::time::timeout(Duration::from_millis(100), receive.recv()).await;
    assert!(
        gossip_validity_notified.is_ok(),
        "expected to receive gossip validity notification"
    )
}

#[tokio::test]
async fn test_gossipped_mint_rejected() {
    // verify that gossipped mint transactions are rejected (punished)
    let tx1 = TransactionBuilder::mint(
        0u32.into(),
        0,
        Default::default(),
        Default::default(),
        1,
        AssetId::BASE,
        Default::default(),
    )
    .finalize_as_transaction();
    // setup p2p mock - with tx incoming from p2p
    let txs = vec![tx1.clone()];
    let mut p2p = MockP2P::new_with_txs(txs);
    let (send, mut receive) = broadcast::channel::<()>(1);
    p2p.expect_notify_gossip_transaction_validity()
        .returning(move |_, validity| {
            // Expect the transaction to be rejected
            assert_eq!(validity, GossipsubMessageAcceptance::Reject);
            // Notify test that the gossipsub acceptance was set
            send.send(()).unwrap();
            Ok(())
        });
    // setup test context
    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_p2p(p2p);

    // build and start the txpool service
    let ctx = ctx_builder.build();
    let service = ctx.service();
    service.start_and_await().await.unwrap();
    // verify p2p was notified about the transaction validity
    let gossip_validity_notified =
        tokio::time::timeout(Duration::from_millis(100), receive.recv()).await;
    assert!(
        gossip_validity_notified.is_ok(),
        "expected to receive gossip validity notification"
    )
}

#[tokio::test]
async fn test_gossipped_transaction_with_transient_error_ignored() {
    // verify that gossipped transactions that fails stateful checks are ignored (but not punished)
    let mut rng = StdRng::seed_from_u64(100);
    let mut ctx_builder = TestContextBuilder::new();
    // add coin to builder db and generate a valid tx
    let mut tx1 = ctx_builder.setup_script_tx(10);
    // now intentionally muck up the tx such that it will return a coin not found error
    // by replacing the default coin with one that is not in the database
    let script = tx1.as_script_mut().unwrap();
    script.inputs_mut()[0] = crate::test_helpers::random_predicate(
        &mut rng,
        AssetId::BASE,
        TEST_COIN_AMOUNT,
        None,
    );
    // setup p2p mock - with tx incoming from p2p
    let txs = vec![tx1.clone()];
    let mut p2p = MockP2P::new_with_txs(txs);
    let (send, mut receive) = broadcast::channel::<()>(1);
    p2p.expect_notify_gossip_transaction_validity()
        .returning(move |_, validity| {
            // Expect the transaction to be rejected
            assert_eq!(validity, GossipsubMessageAcceptance::Ignore);
            // Notify test that the gossipsub acceptance was set
            send.send(()).unwrap();
            Ok(())
        });
    ctx_builder.with_p2p(p2p);

    // build and start the txpool service
    let ctx = ctx_builder.build();
    let service = ctx.service();
    service.start_and_await().await.unwrap();
    // verify p2p was notified about the transaction validity
    let gossip_validity_notified =
        tokio::time::timeout(Duration::from_millis(100), receive.recv()).await;
    assert!(
        gossip_validity_notified.is_ok(),
        "expected to receive gossip validity notification"
    )
}
