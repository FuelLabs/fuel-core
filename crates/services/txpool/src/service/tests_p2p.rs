use super::*;
use crate::service::test_helpers::{
    MockP2PAdapter,
    TestContextBuilder,
};
use fuel_core_interfaces::txpool::TxPoolMpsc;
use fuel_core_types::fuel_tx::{
    Transaction,
    UniqueIdentifier,
};
use std::{
    ops::Deref,
    time::Duration,
};
use tokio::{
    sync::oneshot,
    time::timeout,
};

#[tokio::test]
async fn can_insert_from_p2p() {
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);

    let p2p_adapter = MockP2PAdapter::new(vec![tx1.clone()]);
    ctx_builder.with_p2p(p2p_adapter);

    let ctx = ctx_builder.build().await;
    let service = ctx.service();

    let mut receiver = service.tx_update_subscribe();
    let res = receiver.recv().await;
    assert!(res.is_ok());

    // fetch tx from pool
    let (response, receiver) = oneshot::channel();
    let _ = service
        .sender()
        .send(TxPoolMpsc::Find {
            ids: vec![tx1.id()],
            response,
        })
        .await;
    let out = receiver.await.unwrap();

    let got_tx: Transaction = out[0].as_ref().unwrap().tx().clone().deref().into();
    assert_eq!(tx1, got_tx);
}

#[tokio::test]
async fn insert_from_local_broadcasts_to_p2p() {
    // setup initial state
    let mut ctx_builder = TestContextBuilder::new();
    // add coin to builder db and generate a valid tx
    let tx1 = ctx_builder.setup_script_tx(10);
    // setup p2p mock
    let p2p_adapter = MockP2PAdapter::new(vec![]);
    let mut broadcasted_txs = p2p_adapter.broadcast_subscribe();
    ctx_builder.with_p2p(p2p_adapter);
    // build and start the txpool service
    let ctx = ctx_builder.build().await;

    let service = ctx.service();
    let mut subscribe_status = service.tx_status_subscribe();
    let mut subscribe_update = service.tx_update_subscribe();

    let (response, receiver) = oneshot::channel();
    let _ = service
        .sender()
        .send(TxPoolMpsc::Insert {
            txs: vec![Arc::new(tx1.clone())],
            response,
        })
        .await;
    let out = receiver.await.unwrap();

    if let Ok(result) = &out[0] {
        // we are sure that included tx are already broadcasted.

        // verify status updates
        assert_eq!(
            subscribe_status.try_recv(),
            Ok(TxStatus::Submitted),
            "First added should be tx1"
        );
        let update = subscribe_update.try_recv().unwrap();
        assert_eq!(
            *update.tx_id(),
            result.inserted.id(),
            "First added should be tx1"
        );

        // verify p2p received tx
        let p2p_broadcast = broadcasted_txs.recv().await.unwrap();
        assert_eq!(p2p_broadcast.id(), tx1.id())
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
    let p2p_adapter = MockP2PAdapter::new(vec![tx1]);
    let mut broadcasted_txs = p2p_adapter.broadcast_subscribe();
    ctx_builder.with_p2p(p2p_adapter);
    // build and start the txpool service
    let ctx = ctx_builder.build().await;
    let service = ctx.service();

    // verify tx status update from p2p injected tx is successful
    let mut receiver = service.tx_update_subscribe();
    let res = receiver.recv().await;
    assert!(res.is_ok());

    // verify tx was not broadcast to p2p
    let not_broadcast = timeout(Duration::from_millis(100), broadcasted_txs.recv()).await;
    assert!(
        not_broadcast.is_err(),
        "expected a timeout because no broadcast should have occurred"
    )
}
