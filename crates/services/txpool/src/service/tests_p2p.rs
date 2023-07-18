use super::*;
use crate::service::test_helpers::{
    MockP2P,
    TestContextBuilder,
};
use fuel_core_services::Service;
use fuel_core_types::fuel_tx::{
    Transaction,
    UniqueIdentifier,
};
use std::{
    ops::Deref,
    time::Duration,
};

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
        .tx_update_subscribe(tx1.id(&ConsensusParameters::DEFAULT.chain_id))
        .await;

    service.start_and_await().await.unwrap();

    let res = receiver.next().await;
    assert!(matches!(
        res,
        Some(TxStatusMessage::Status(TransactionStatus::Submitted { .. }))
    ));

    // fetch tx from pool
    let out = service
        .shared
        .find(vec![tx1.id(&ConsensusParameters::DEFAULT.chain_id)]);

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
        .await;

    let out = service.shared.insert(vec![Arc::new(tx1.clone())]);

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
        .tx_update_subscribe(tx1.id(&ConsensusParameters::DEFAULT.chain_id))
        .await;

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
