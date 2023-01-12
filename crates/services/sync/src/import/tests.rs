use fuel_core_services::{
    KillSwitch,
    PeerId,
};
use fuel_core_types::{
    blockchain::{
        consensus::Consensus,
        header::BlockHeader,
    },
    services::executor::ExecutionResult,
};

use crate::ports::{
    MockExecutor,
    MockPeerToPeer,
};
use test_case::test_case;

use super::*;

#[test_case(
    State::test(None, Some(5), None), 6 => State::test(None, Some(5), Some(5))
)]
#[test_case(
    State::test(None, Some(5), Some(0)), 5 => State::test(None, Some(5), Some(5))
)]
#[tokio::test]
async fn test_import(state: State, expected_executes: usize) -> State {
    let state = SharedMutex::new(state);
    let notify = Arc::new(Notify::new());
    let params = Params {
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
    };
    let mut p2p = MockPeerToPeer::default();

    p2p.expect_get_sealed_block_header()
        .times(expected_executes)
        .returning(|h| {
            let mut header = BlockHeader::default();
            header.consensus.height = h;

            let consensus = Consensus::default();
            let sealed = Sealed {
                entity: header,
                consensus,
            };
            let peer_id = PeerId {};
            let source_peer = SourcePeer {
                peer_id,
                data: sealed,
            };
            Ok(Some(source_peer))
        });
    p2p.expect_get_transactions()
        .returning(|_| Ok(Some(vec![])));
    let p2p = Arc::new(p2p);
    let mut executor = MockExecutor::default();

    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    let mut count = 0;
    executor.expect_execute_and_commit().returning(move |_| {
        count += 1;
        if count == expected_executes {
            tx.take().unwrap().send(()).unwrap();
        }
        Ok(ExecutionResult {
            block: Block::default(),
            skipped_transactions: vec![],
            tx_status: vec![],
        })
    });
    let executor = Arc::new(executor);
    let mut ks = KillSwitch::new();
    let jh = tokio::spawn(import(
        state.clone(),
        notify,
        params,
        p2p,
        executor,
        ks.handle(),
    ));
    rx.await.unwrap();
    ks.kill_all();
    jh.await.unwrap();
    state.apply(|s| s.clone())
}
