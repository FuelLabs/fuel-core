#![allow(clippy::arithmetic_side_effects)]
#![allow(non_snake_case)]

use crate::{
    import::test_helpers::{
        empty_header,
        random_peer,
    },
    ports::{
        MockBlockImporterPort,
        MockConsensusPort,
        MockPeerToPeerPort,
        PeerReportReason,
    },
};
use fuel_core_types::services::p2p::Transactions;
use mockall::Sequence;
use std::{
    ops::Deref,
    time::Duration,
};

use super::*;

fn div_ceil(divisor: usize, dividend: usize) -> usize {
    (divisor + (dividend - 1)) / dividend
}

#[tokio::test]
async fn test_import_0_to_5() {
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(6)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([6]),
    };

    let state = State::new(None, 5);
    let state = SharedMutex::new(state);
    let v = test_import_inner(state, mocks, None, params).await;
    let expected = (State::new(5, None), true);
    assert_eq!(v, expected);
}

#[tokio::test]
async fn test_import_3_to_5() {
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([2]),
    };

    let state = State::new(3, 5);
    let state = SharedMutex::new(state);
    let v = test_import_inner(state, mocks, None, params).await;
    let expected = (State::new(5, None), true);
    assert_eq!(v, expected);
}

#[tokio::test]
async fn test_import_0_to_499() {
    // The observed block height
    let end_u32: u32 = 499;
    let end = end_u32 as usize;
    // The number of headers/blocks in range 0..end
    let n = end + 1;
    // The number of headers/blocks per batch
    let header_batch_size = 10;

    let mut consensus_port = MockConsensusPort::default();

    // Happens once for each header
    let times = n;
    consensus_port
        .expect_check_sealed_header()
        .times(times)
        .returning(|_| Ok(true));

    // Happens once for each batch
    let times = div_ceil(n, header_batch_size);
    consensus_port
        .expect_await_da_height()
        .times(times)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();

    // Happens once for each batch
    let times = div_ceil(n, header_batch_size);
    p2p.expect_get_sealed_block_headers()
        .times(times)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });

    // Happens once for each batch
    let times = div_ceil(n, header_batch_size);
    p2p.expect_get_transactions_from_peer()
        .times(times)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size,
    };
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([n]),
    };

    let state = State::new(None, end_u32);
    let state = SharedMutex::new(state);
    let v = test_import_inner(state, mocks, None, params).await;
    let expected = (State::new(end_u32, None), true);
    assert_eq!(v, expected);
}

#[tokio::test]
async fn import__signature_fails_on_header_5_only() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|h| Ok(**h.entity.height() != 5));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([1]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__keep_data_asked_in_fail_ask_header_cases() {
    // Test is going from block 4 (3 already committed) to 6
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 1,
    };

    let mut consensus_port = MockConsensusPort::default();
    // No reask on verification on all of the blocks
    consensus_port
        .expect_check_sealed_header()
        .times(3)
        .returning(|_| Ok(true));
    // No reask on da height on all of the blocks
    consensus_port
        .expect_await_da_height()
        .times(3)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    let mut seq = Sequence::new();
    // Given
    // Fail to get headers for block 4
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|range| {
            assert_eq!(range, 4..5);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Err(anyhow::anyhow!("Some network error"))
            })
        });
    // Success for 5 and 6 that is in parallel with 4
    p2p.expect_get_sealed_block_headers()
        .times(2)
        .in_sequence(&mut seq)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    // Then
    // Reask only for block 4
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|range| {
            assert_eq!(range, 4..5);
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    // No reask on getting full block step for 4, 5 and 6 blocks
    p2p.expect_get_transactions_from_peer()
        .times(3)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });
    p2p.expect_report_peer().returning(|_, _| Ok(()));

    let p2p = Arc::new(p2p);
    let executor: Arc<MockBlockImporterPort> = Arc::new(DefaultMocks::times([3]));
    let consensus = Arc::new(consensus_port);
    let notify = Arc::new(Notify::new());
    let state: SharedMutex<State> = State::new(3, 6).into();

    let mut import = Import {
        state: state.clone(),
        notify: notify.clone(),
        params,
        p2p,
        executor,
        consensus,
        cache: Cache::new(),
    };
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();
    notify.notify_one();
    let res = import.import(&mut watcher).await.is_ok();
    assert!(!res);
    assert_eq!(&State::new(3, None), state.lock().deref());
    // Reset the state for a next call
    *state.lock() = State::new(3, 6);

    // When
    // Should re-ask to P2P only block 4.
    let res = import.import(&mut watcher).await.is_ok();
    assert!(res);
    assert_eq!(&State::new(6, None), state.lock().deref());
}

#[tokio::test]
async fn import__keep_data_asked_in_fail_ask_transactions_cases() {
    // Test is going from block 4 (3 already committed) to 6
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 1,
    };

    let mut consensus_port = MockConsensusPort::default();
    // No reask on verification on all of the blocks
    consensus_port
        .expect_check_sealed_header()
        .times(3)
        .returning(|_| Ok(true));
    // One reask on the da_height after reask of the transactions for block 4
    consensus_port
        .expect_await_da_height()
        .times(4)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    // Everything goes well on the headers part for all blocks
    p2p.expect_get_sealed_block_headers()
        .times(3)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    let mut seq = Sequence::new();
    // Given
    // Fail to get transactions for block 4
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|range| {
            assert_eq!(range.data, 4..5);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Err(anyhow::anyhow!("Some network error"))
            })
        });

    // Success for 5 and 6 that is in parallel with 4
    p2p.expect_get_transactions_from_peer()
        .times(2)
        .in_sequence(&mut seq)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });
    // Then
    // Reask only for block 4
    p2p.expect_get_transactions()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|block_ids| {
            assert_eq!(block_ids, 4..5);
            Box::pin(async move {
                let data = block_ids;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(SourcePeer {
                    peer_id: random_peer(),
                    data: Some(v),
                })
            })
        });

    p2p.expect_report_peer().returning(|_, _| Ok(()));

    let p2p = Arc::new(p2p);
    let executor: Arc<MockBlockImporterPort> = Arc::new(DefaultMocks::times([3]));
    let consensus = Arc::new(consensus_port);
    let notify = Arc::new(Notify::new());
    let state: SharedMutex<State> = State::new(3, 6).into();

    let mut import = Import {
        state: state.clone(),
        notify: notify.clone(),
        params,
        p2p,
        executor,
        consensus,
        cache: Cache::new(),
    };
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();
    notify.notify_one();
    let res = import.import(&mut watcher).await.is_ok();
    assert!(!res);
    assert_eq!(&State::new(3, None), state.lock().deref());
    // Reset the state for a next call
    *state.lock() = State::new(3, 6);
    // When
    // Should re-ask to P2P only block 4.
    let res = import.import(&mut watcher).await.is_ok();
    assert!(res);
    assert_eq!(&State::new(6, None), state.lock().deref());
}

#[tokio::test]
async fn import__keep_data_asked_in_fail_execution() {
    // Test is going from block 4 (3 already committed) to 6
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 1,
    };

    let mut consensus_port = MockConsensusPort::default();
    // Data is re-ask for the block 4 because his execution failed
    consensus_port
        .expect_check_sealed_header()
        .times(4)
        .returning(|_| Ok(true));
    // Data is re-ask for the block 4 because his execution failed
    consensus_port
        .expect_await_da_height()
        .times(4)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    // Data is re-ask for the block 4 because his execution failed
    p2p.expect_get_sealed_block_headers()
        .times(4)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });

    // Data is re-ask for the block 4 because his execution failed
    p2p.expect_get_transactions_from_peer()
        .times(4)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });
    p2p.expect_report_peer().returning(|_, _| Ok(()));

    let p2p = Arc::new(p2p);

    // Given
    let mut executor: MockBlockImporterPort = MockBlockImporterPort::new();
    let mut seq = Sequence::new();
    // Fails execute on the 4 one
    executor
        .expect_execute_and_commit()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|block| {
            Box::pin(async move {
                assert_eq!(block.entity.header().height(), &BlockHeight::new(4));
                anyhow::bail!("Bad execution")
            })
        });
    // Success execute the 3 after
    executor
        .expect_execute_and_commit()
        .times(3)
        .in_sequence(&mut seq)
        .returning(|_| Box::pin(async move { Ok(()) }));
    let executor = Arc::new(executor);
    let consensus = Arc::new(consensus_port);
    let notify = Arc::new(Notify::new());
    let state: SharedMutex<State> = State::new(3, 6).into();

    let mut import = Import {
        state: state.clone(),
        notify: notify.clone(),
        params,
        p2p,
        executor,
        consensus,
        cache: Cache::new(),
    };
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();
    notify.notify_one();
    let res = import.import(&mut watcher).await.is_ok();
    assert!(!res);
    assert_eq!(&State::new(3, None), state.lock().deref());
    // Reset the state for a next call
    *state.lock() = State::new(3, 6);
    // When
    // Should re-ask to P2P only block 4.
    let res = import.import(&mut watcher).await.is_ok();
    assert!(res);
    assert_eq!(&State::new(6, None), state.lock().deref());
}

#[tokio::test]
async fn import__signature_fails_on_header_4_only() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(1)
        .returning(|h| Ok(**h.entity.height() != 4));
    consensus_port
        .expect_await_da_height()
        .times(0)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(0)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__header_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(Vec::new());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__header_response_incomplete() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = None;
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__header_5_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(vec![empty_header(4)]);
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });

    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([1]),
        executor: DefaultMocks::times([1]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__header_4_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(vec![empty_header(5)]);
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer().times(0);

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__transactions_not_found() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(None) }));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port,
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__transactions_not_found_for_header_4() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    let mut height = 3;
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(move |block_ids| {
            Box::pin(async move {
                height += 1;
                if height == 4 {
                    Ok(None)
                } else {
                    let data = block_ids.data;
                    let v = data.into_iter().map(|_| Transactions::default()).collect();
                    Ok(Some(v))
                }
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port,
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__transactions_not_found_for_header_5() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(move |_| {
            Box::pin(async move {
                let v = vec![Transactions::default()];
                Ok(Some(v))
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port,
        executor: DefaultMocks::times([1]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__p2p_error() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Box::pin(async move { Err(anyhow::anyhow!("Some network error")) })
        });
    p2p.expect_get_transactions_from_peer().times(0);

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__p2p_error_on_4_transactions() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|_| {
            Box::pin(async move { Err(anyhow::anyhow!("Some network error")) })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port,
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__consensus_error_on_4() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(1)
        .returning(|h| {
            if **h.entity.height() == 4 {
                Err(anyhow::anyhow!("Some consensus error"))
            } else {
                Ok(true)
            }
        });
    consensus_port
        .expect_await_da_height()
        .times(0)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer().times(0);

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__consensus_error_on_5() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|h| {
            if **h.entity.height() == 5 {
                Err(anyhow::anyhow!("Some consensus error"))
            } else {
                Ok(true)
            }
        });
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([1]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__execution_error_on_header_4() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let mut executor = MockBlockImporterPort::default();
    executor
        .expect_execute_and_commit()
        .times(1)
        .returning(|h| {
            Box::pin(async move {
                if **h.entity.header().height() == 4 {
                    Err(anyhow::anyhow!("Some execution error"))
                } else {
                    Ok(())
                }
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor,
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__execution_error_on_header_5() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|range| {
            Box::pin(async move {
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(1)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let mut executor = MockBlockImporterPort::default();
    executor
        .expect_execute_and_commit()
        .times(2)
        .returning(|h| {
            Box::pin(async move {
                if **h.entity.header().height() == 5 {
                    Err(anyhow::anyhow!("Some execution error"))
                } else {
                    Ok(())
                }
            })
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor,
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn signature_always_fails() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(1)
        .returning(|_| Ok(false));
    consensus_port.expect_await_da_height().times(0);

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(state, mocks, None, params).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__can_work_in_two_loops() {
    // given
    let s = SharedMutex::new(State::new(3, 5));
    let state = s.clone();

    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(3)
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .times(2)
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(2)
        .returning(move |range| {
            let state = state.clone();
            Box::pin(async move {
                state.apply(|s| s.observe(6));
                let peer = random_peer();
                let headers = Some(range.map(empty_header).collect());
                let headers = peer.bind(headers);
                Ok(headers)
            })
        });
    p2p.expect_get_transactions_from_peer()
        .times(2)
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let c = DefaultMocks::times([2]);
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor: DefaultMocks::times([3]),
    };
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };

    // when
    let res = test_import_inner(s, mocks, Some(c), params).await;

    // then
    assert_eq!((State::new(6, None), true), res);
}

async fn test_import_inner(
    state: SharedMutex<State>,
    mocks: Mocks,
    count: Option<Count>,
    params: Config,
) -> (State, bool) {
    let notify = Arc::new(Notify::new());
    let Mocks {
        consensus_port,
        mut p2p,
        executor,
    } = mocks;
    p2p.expect_report_peer().returning(|_, _| Ok(()));
    let p2p = Arc::new(p2p);

    let executor = Arc::new(executor);
    let consensus = Arc::new(consensus_port);

    let mut import = Import {
        state,
        notify,
        params,
        p2p,
        executor,
        consensus,
        cache: Cache::new(),
    };
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();
    let received_notify_signal = match count {
        Some(Count(count)) => {
            let mut r = false;
            for _ in 0..count {
                import.notify.notify_one();
                r = import.import(&mut watcher).await.is_ok();
                if !r {
                    break
                }
            }
            r
        }
        None => {
            import.notify.notify_one();
            import.import(&mut watcher).await.is_ok()
        }
    };
    let final_state = import.state.apply(|s| s.clone());
    (final_state, received_notify_signal)
}

#[tokio::test]
async fn import__happy_path_sends_good_peer_report() {
    // Given
    PeerReportTestBuilder::new()
        // When (no changes)
        // Then
        .run_with_expected_reports([PeerReportReason::SuccessfulBlockImport])
        .await;
}

#[tokio::test]
async fn import__multiple_blocks_happy_path_sends_good_peer_report() {
    // Given
    PeerReportTestBuilder::new()
        // When 
        .times(3)
        // Then
        .run_with_expected_reports([PeerReportReason::SuccessfulBlockImport])
        .await;
}

#[tokio::test]
async fn import__missing_headers_sends_peer_report() {
    // Given
    PeerReportTestBuilder::new()
        // When
        .with_get_sealed_block_headers(None)
        // Then
        .run_with_expected_reports([PeerReportReason::MissingBlockHeaders])
        .await;
}

#[tokio::test]
async fn import__bad_block_header_sends_peer_report() {
    // Given
    PeerReportTestBuilder::new()
        // When
        .with_check_sealed_header(false)
        // Then
        .run_with_expected_reports([PeerReportReason::BadBlockHeader])
        .await;
}

#[tokio::test]
async fn import__missing_transactions_sends_peer_report() {
    // Given
    PeerReportTestBuilder::new()
        // When
        .with_get_transactions(None)
        // Then
        .run_with_expected_reports([PeerReportReason::MissingTransactions])
        .await;
}

#[tokio::test]
async fn import__execution_error_on_header_4_when_awaits_for_1000000_blocks() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .returning(|_| Ok(true));
    consensus_port
        .expect_await_da_height()
        .returning(|_| Box::pin(async move { Ok(()) }));

    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers().returning(|range| {
        Box::pin(async move {
            let peer = random_peer();
            let headers = Some(range.map(empty_header).collect());
            let headers = peer.bind(headers);
            Ok(headers)
        })
    });
    p2p.expect_get_transactions_from_peer()
        .returning(|block_ids| {
            Box::pin(async move {
                let data = block_ids.data;
                let v = data.into_iter().map(|_| Transactions::default()).collect();
                Ok(Some(v))
            })
        });

    let mut executor = MockBlockImporterPort::default();
    executor
        .expect_execute_and_commit()
        .times(1)
        .returning(|h| {
            Box::pin(async move {
                if **h.entity.header().height() == 4 {
                    Err(anyhow::anyhow!("Some execution error"))
                } else {
                    Ok(())
                }
            })
        });

    let state = State::new(3, 1000000).into();
    let mocks = Mocks {
        consensus_port,
        p2p,
        executor,
    };
    let params = Config {
        block_stream_buffer_size: 1,
        header_batch_size: 1,
    };

    // when
    let res = tokio::time::timeout(
        Duration::from_secs(1),
        test_import_inner(state, mocks, None, params),
    )
    .await;

    // then
    let res = res.expect("Should not timeout if the first block failed execution");
    assert_eq!((State::new(3, None), false), res);
}

struct PeerReportTestBuilder {
    shared_peer_id: Vec<u8>,
    get_sealed_headers: Option<Option<Vec<SealedBlockHeader>>>,
    get_transactions: Option<Option<Vec<Transactions>>>,
    check_sealed_header: Option<bool>,
    block_count: u32,
    debug: bool,
}

impl PeerReportTestBuilder {
    pub fn new() -> Self {
        Self {
            shared_peer_id: vec![1, 2, 3, 4],
            get_sealed_headers: None,
            get_transactions: None,
            check_sealed_header: None,
            block_count: 1,
            debug: false,
        }
    }

    #[allow(dead_code)]
    pub fn debug(mut self) -> Self {
        self.debug = true;
        self
    }

    pub fn with_get_sealed_block_headers(
        mut self,
        get_headers: Option<Vec<SealedBlockHeader>>,
    ) -> Self {
        self.get_sealed_headers = Some(get_headers);
        self
    }

    pub fn with_get_transactions(
        mut self,
        get_transactions: Option<Vec<Transactions>>,
    ) -> Self {
        self.get_transactions = Some(get_transactions);
        self
    }

    pub fn with_check_sealed_header(mut self, check_sealed_header: bool) -> Self {
        self.check_sealed_header = Some(check_sealed_header);
        self
    }

    pub fn times(mut self, block_count: u32) -> Self {
        self.block_count = block_count;
        self
    }

    pub async fn run_with_expected_reports<R>(self, expected_reports: R)
    where
        R: IntoIterator<Item = PeerReportReason>,
    {
        if self.debug {
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        }

        let p2p = self.p2p(expected_reports);
        let executor = self.executor();
        let consensus = self.consensus();

        let index = self.block_count - 1;
        let state = State::new(None, index).into();

        let notify = Arc::new(Notify::new());

        let params = Config {
            block_stream_buffer_size: 10,
            header_batch_size: 10,
        };

        let mut import = Import {
            state,
            notify,
            params,
            p2p,
            executor,
            consensus,
            cache: Cache::new(),
        };
        let (_tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher = shutdown.into();

        import.notify.notify_one();
        let _ = import.import(&mut watcher).await;
    }

    fn p2p<R>(&self, expected_reports: R) -> Arc<MockPeerToPeerPort>
    where
        R: IntoIterator<Item = PeerReportReason>,
    {
        let mut p2p = MockPeerToPeerPort::default();

        let peer_id = self.shared_peer_id.clone();
        if let Some(get_headers) = self.get_sealed_headers.clone() {
            p2p.expect_get_sealed_block_headers().returning(move |_| {
                let peer: PeerId = peer_id.clone().into();
                let get_headers = get_headers.clone();
                Box::pin(async move {
                    let headers = peer.bind(get_headers);
                    Ok(headers)
                })
            });
        } else {
            p2p.expect_get_sealed_block_headers()
                .returning(move |range| {
                    let peer: PeerId = peer_id.clone().into();
                    Box::pin(async move {
                        let headers = Some(range.map(empty_header).collect());
                        let headers = peer.bind(headers);
                        Ok(headers)
                    })
                });
        }

        let transactions = self.get_transactions.clone();
        if let Some(t) = transactions {
            p2p.expect_get_transactions_from_peer().returning(move |_| {
                let t = t.clone();
                Box::pin(async move { Ok(t) })
            });
        } else {
            p2p.expect_get_transactions_from_peer()
                .returning(|block_ids| {
                    Box::pin(async move {
                        let data = block_ids.data;
                        let v =
                            data.into_iter().map(|_| Transactions::default()).collect();
                        Ok(Some(v))
                    })
                });
        }

        let mut seq = mockall::Sequence::new();
        let peer_id: PeerId = self.shared_peer_id.clone().into();
        let expected_reports = expected_reports.into_iter();
        for expected_report in expected_reports {
            p2p.expect_report_peer()
                .times(1)
                .with(
                    mockall::predicate::eq(peer_id.clone()),
                    mockall::predicate::eq(expected_report),
                )
                .returning(|_, _| Ok(()))
                .in_sequence(&mut seq);
        }

        Arc::new(p2p)
    }

    fn executor(&self) -> Arc<MockBlockImporterPort> {
        let mut executor = MockBlockImporterPort::default();

        executor
            .expect_execute_and_commit()
            .returning(|_| Box::pin(async move { Ok(()) }));

        Arc::new(executor)
    }

    fn consensus(&self) -> Arc<MockConsensusPort> {
        let mut consensus_port = MockConsensusPort::default();

        consensus_port
            .expect_await_da_height()
            .returning(|_| Box::pin(async move { Ok(()) }));

        let check_sealed_header = self.check_sealed_header.unwrap_or(true);
        consensus_port
            .expect_check_sealed_header()
            .returning(move |_| Ok(check_sealed_header));

        Arc::new(consensus_port)
    }
}

struct Mocks {
    consensus_port: MockConsensusPort,
    p2p: MockPeerToPeerPort,
    executor: MockBlockImporterPort,
}

struct Count(usize);

trait DefaultMocks {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone;
}

impl DefaultMocks for Mocks {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        Self {
            consensus_port: DefaultMocks::times(t.clone()),
            p2p: DefaultMocks::times(t.clone()),
            executor: DefaultMocks::times(t),
        }
    }
}

impl DefaultMocks for Count {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        Self(t.into_iter().next().unwrap())
    }
}

impl DefaultMocks for MockConsensusPort {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        let mut consensus_port = MockConsensusPort::new();
        let mut t = t.into_iter().cycle();
        consensus_port
            .expect_check_sealed_header()
            .times(t.next().unwrap())
            .returning(|_| Ok(true));
        consensus_port
            .expect_await_da_height()
            .times(t.next().unwrap())
            .returning(|_| Box::pin(async move { Ok(()) }));
        consensus_port
    }
}

impl DefaultMocks for MockPeerToPeerPort {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        let mut p2p = MockPeerToPeerPort::default();
        let mut t = t.into_iter().cycle();

        p2p.expect_get_sealed_block_headers()
            .times(1)
            .returning(|range| {
                Box::pin(async move {
                    let peer = random_peer();
                    let headers = Some(range.map(empty_header).collect());
                    let headers = peer.bind(headers);
                    Ok(headers)
                })
            });

        p2p.expect_get_transactions_from_peer()
            .times(t.next().unwrap())
            .returning(|block_ids| {
                Box::pin(async move {
                    let data = block_ids.data;
                    let v = data.into_iter().map(|_| Transactions::default()).collect();
                    Ok(Some(v))
                })
            });
        p2p
    }
}

impl DefaultMocks for MockBlockImporterPort {
    fn times<T: IntoIterator<Item = usize> + Clone>(t: T) -> Self {
        let mut executor = MockBlockImporterPort::default();
        let t = t.into_iter().next().unwrap();

        executor
            .expect_execute_and_commit()
            .times(t)
            .returning(move |_| Box::pin(async move { Ok(()) }));
        executor
    }
}
