use super::*;
use crate::service::test_helpers::TestContext;
use fuel_core_interfaces::txpool::{
    TxPoolMpsc,
    TxStatus,
    TxStatusBroadcast,
};
use tokio::sync::{
    mpsc::error::TryRecvError,
    oneshot,
};

#[tokio::test]
async fn can_insert_from_p2p() {
    let ctx = TestContext::new().await;
    let service = ctx.service();
    let tx1 = ctx.setup_script_tx(10);

    let broadcast_tx = TransactionGossipData::new(
        TransactionBroadcast::NewTransaction(tx1.clone()),
        vec![],
        vec![],
    );
    let mut receiver = service.subscribe_ch();
    let res = ctx.gossip_tx.send(broadcast_tx).unwrap();
    let _ = receiver.recv().await;
    assert_eq!(1, res);

    let (response, receiver) = oneshot::channel();
    let _ = service
        .sender()
        .send(TxPoolMpsc::Find {
            ids: vec![tx1.id()],
            response,
        })
        .await;
    let out = receiver.await.unwrap();

    let arc_tx1 = Arc::new(tx1);

    assert_eq!(arc_tx1, *out[0].as_ref().unwrap().tx());
}

#[tokio::test]
async fn insert_from_local_broadcasts_to_p2p() {
    let ctx = TestContext::new().await;
    let tx1 = Arc::new(ctx.setup_script_tx(10));
    let service = ctx.service();
    let mut subscribe = service.subscribe_ch();

    let (response, receiver) = oneshot::channel();
    let _ = service
        .sender()
        .send(TxPoolMpsc::Insert {
            txs: vec![tx1.clone()],
            response,
        })
        .await;
    let out = receiver.await.unwrap();

    assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);

    // we are sure that included tx are already broadcasted.
    assert_eq!(
        subscribe.try_recv(),
        Ok(TxStatusBroadcast {
            tx: tx1.clone(),
            status: TxStatus::Submitted,
        }),
        "First added should be tx1"
    );

    let ret = ctx.p2p_request_rx.lock().await.recv().await.unwrap();

    if let P2pRequestEvent::BroadcastNewTransaction { transaction } = ret {
        assert_eq!(tx1, transaction);
    } else {
        panic!("Transaction Broadcast Unwrap Failed");
    }
}

#[tokio::test]
async fn test_insert_from_p2p_does_not_broadcast_to_p2p() {
    let ctx = TestContext::new().await;
    let service = ctx.service();

    let tx1 = ctx.setup_script_tx(10);
    let broadcast_tx = TransactionGossipData::new(
        TransactionBroadcast::NewTransaction(tx1.clone()),
        vec![],
        vec![],
    );
    let mut receiver = service.subscribe_ch();
    let res = ctx.gossip_tx.send(broadcast_tx).unwrap();
    let _ = receiver.recv().await;
    assert_eq!(1, res);

    let ret = ctx.p2p_request_rx.lock().await.try_recv();
    assert!(ret.is_err());
    assert_eq!(Some(TryRecvError::Empty), ret.err());
}
