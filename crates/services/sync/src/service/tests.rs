use fuel_core_services::{
    stream::IntoBoxStream,
    Service,
};
use futures::{
    stream,
    StreamExt,
};

use crate::{
    import::test_helpers::empty_header,
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
        Ok(Some(
            range
                .clone()
                .map(BlockHeight::from)
                .map(empty_header)
                .collect(),
        ))
    });
    p2p.expect_get_transactions()
        .returning(|_| Ok(Some(vec![])));
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
        max_get_txns_requests: 10,
        header_batch_size: 10,
        max_header_batch_requests: 10,
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
