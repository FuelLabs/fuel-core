use std::time::Duration;

use fuel_core_services::PeerId;
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

use super::*;

#[tokio::test]
async fn test_import() {
    let state = SharedMutex::new(State {
        best_seen_height: Some(5u32.into()),
        ..Default::default()
    });
    let notify = Arc::new(Notify::new());
    let params = Params {
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
    };
    let mut p2p = MockPeerToPeer::default();
    p2p.expect_get_sealed_block_header().returning(|_| {
        let header = BlockHeader::default();
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
    executor.expect_execute_and_commit().returning(|_| {
        Ok(ExecutionResult {
            block: Block::default(),
            skipped_transactions: vec![],
            tx_status: vec![],
        })
    });
    let executor = Arc::new(executor);
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        import(state, notify, params, p2p, executor),
    )
    .await;
}
