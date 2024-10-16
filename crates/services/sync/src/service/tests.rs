use fuel_core_services::{
    stream::IntoBoxStream,
    Service,
};
use fuel_core_types::services::p2p::Transactions;
use futures::{
    stream,
    StreamExt,
};

use crate::{
    import::test_helpers::{
        empty_header,
        random_peer,
    },
    ports::{
        MockBlockImporterPort,
        MockConsensusPort,
        MockPeerToPeerPort,
    },
};

use super::*;

#[tokio::test]
async fn test_new_service() {
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_report_peer().returning(|_, _| Ok(()));
    p2p.expect_height_stream().returning(|| {
        stream::iter(
            std::iter::successors(Some(6u32), |n| Some(n + 1)).map(BlockHeight::from),
        )
        .then(|h| async move {
            if *h == 17 {
                futures::future::pending::<()>().await;
            }
            h
        })
        .into_boxed()
    });
    p2p.expect_get_sealed_block_headers().returning(|range| {
        Box::pin(async move {
            let peer = random_peer();
            let headers = Some(range.map(empty_header).collect::<Vec<_>>());
            let headers = peer.bind(headers);
            Ok(headers)
        })
    });
    p2p.expect_get_transactions().returning(|block_ids| {
        Box::pin(async move {
            let data = block_ids.data;
            let v = data.into_iter().map(|_| Transactions::default()).collect();
            Ok(Some(v))
        })
    });
    let mut importer = MockBlockImporterPort::default();
    importer
        .expect_committed_height_stream()
        .returning(|| futures::stream::pending::<BlockHeight>().into_boxed());
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    importer.expect_execute_and_commit().returning(move |h| {
        tx.try_send(**h.entity.header().height()).unwrap();
        Ok(())
    });
    let mut consensus = MockConsensusPort::default();
    consensus
        .expect_check_sealed_header()
        .returning(|_| Ok(true));
    consensus.expect_await_da_height().returning(|_| Ok(()));
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };
    let s = new_service(4u32.into(), p2p, importer, consensus, params).unwrap();

    assert_eq!(
        s.start_and_await().await.unwrap(),
        fuel_core_services::State::Started
    );
    let mut last_value = 0;
    while let Some(h) = rx.recv().await {
        last_value = h;
        if h == 16 {
            break
        }
    }

    assert_eq!(last_value, 16);
    assert_eq!(
        s.stop_and_await().await.unwrap(),
        fuel_core_services::State::Stopped
    );
}
