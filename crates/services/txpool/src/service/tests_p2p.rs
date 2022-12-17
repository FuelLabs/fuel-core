use super::*;
use crate::service::test_helpers::{
    MockP2PAdapter,
    TestContext,
    TestContextBuilder,
};
use fuel_core_interfaces::txpool::TxPoolMpsc;
use fuel_core_types::fuel_tx::{
    Transaction,
    UniqueIdentifier,
};
use mockall::predicate::eq;
use std::ops::Deref;
use tokio::sync::oneshot;

#[tokio::test]
async fn can_insert_from_p2p() {
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);

    let mut p2p_adapter = MockP2PAdapter::default();
    let mock_tx1 = tx1.clone();
    p2p_adapter
        .expect_next_gossiped_transaction()
        .returning(move || GossipData::new(mock_tx1.clone(), vec![], vec![]));
    ctx_builder.with_p2p(p2p_adapter);

    let ctx = ctx_builder.build().await;
    let service = ctx.service();
    let tx1 = ctx.setup_script_tx(10);

    let mut receiver = service.tx_update_subscribe();
    let res = receiver.recv().await;
    assert!(res.is_ok());

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
    let mut ctx_builder = TestContextBuilder::new();
    let tx1 = ctx_builder.setup_script_tx(10);

    let mut p2p_adapter = MockP2PAdapter::default();
    let mock_tx1 = tx1.clone();
    p2p_adapter
        .expect_next_gossiped_transaction()
        .returning(move || {
            tokio::task::yield_now();
            GossipData::new(mock_tx1.clone(), vec![], vec![])
        });
    let mock_tx1 = tx1.clone();
    p2p_adapter
        .expect_broadcast_transaction()
        .times(1)
        .withf(move |tx: &Arc<Transaction>| **tx == mock_tx1);
    ctx_builder.with_p2p(p2p_adapter);

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
    } else {
        panic!("Tx1 should be OK, got err");
    }
}

// #[tokio::test]
// async fn test_insert_from_p2p_does_not_broadcast_to_p2p() {
//     let ctx_builder = TestContextBuilder::new();
//
//     let ctx = TestContext::new().await;
//     let service = ctx.service();
//
//     let tx1 = ctx.setup_script_tx(10);
//     ctx.p2p
//         .expect_next_gossiped_transaction()
//         .returning(|| GossipData::new(tx1.clone(), vec![], vec![]));
//     let mut receiver = service.tx_update_subscribe();
//     let res = receiver.recv().await;
//     assert!(res.is_ok());
// }
