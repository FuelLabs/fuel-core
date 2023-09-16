#![allow(non_snake_case)]

use crate::{
    import::test_helpers::{
        empty_header,
        peer_sourced_headers,
        peer_sourced_headers_peer_id,
    },
    ports::{
        MockBlockImporterPort,
        MockConsensusPort,
        MockPeerToPeerPort,
        PeerReportReason,
    },
};
use fuel_core_types::fuel_tx::Transaction;
use test_case::test_case;

use super::*;

#[test_case(State::new(None, 5), Mocks::times([6]) => (State::new(5, None), true) ; "executes 5")]
#[test_case(State::new(3, 5), Mocks::times([2]) => (State::new(5, None), true) ; "executes 3 to 5")]
#[tokio::test]
async fn test_import(state: State, mocks: Mocks) -> (State, bool) {
    let state = SharedMutex::new(state);
    test_import_inner(state, mocks, None).await
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
        .returning(|_| Ok(()));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([1]),
        executor: DefaultMocks::times([1]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), true), res);
}

#[tokio::test]
async fn import__signature_fails_on_header_4_only() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|h| Ok(**h.entity.height() != 4));
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Ok(()));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([1]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__header_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| Ok(peer_sourced_headers(Some(Vec::new()))));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__header_response_incomplete() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| Ok(peer_sourced_headers(None)));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__header_5_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| Ok(peer_sourced_headers(Some(vec![empty_header(4.into())]))));
    p2p.expect_get_transactions()
        .times(1)
        .returning(|_| Ok(Some(vec![])));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([1]),
        executor: DefaultMocks::times([1]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), true), res);
}

#[tokio::test]
async fn import__header_4_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| Ok(peer_sourced_headers(Some(vec![empty_header(5.into())]))));
    p2p.expect_get_transactions()
        .times(0)
        .returning(|_| Ok(Some(vec![])));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__transactions_not_found() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Ok(peer_sourced_headers(Some(vec![
                empty_header(4.into()),
                empty_header(5.into()),
            ])))
        });
    p2p.expect_get_transactions()
        .times(2)
        .returning(|_| Ok(None));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([2]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__transactions_not_found_for_header_4() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Ok(peer_sourced_headers(Some(vec![
                empty_header(4.into()),
                empty_header(5.into()),
            ])))
        });
    let mut height = 3;
    p2p.expect_get_transactions().times(2).returning(move |_| {
        height += 1;
        if height == 4 {
            Ok(None)
        } else {
            Ok(Some(vec![]))
        }
    });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([2]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__transactions_not_found_for_header_5() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Ok(peer_sourced_headers(Some(vec![
                empty_header(4.into()),
                empty_header(5.into()),
            ])))
        });
    let mut height = 3;
    p2p.expect_get_transactions().times(2).returning(move |_| {
        height += 1;
        if height == 5 {
            Ok(None)
        } else {
            Ok(Some(vec![]))
        }
    });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([2]),
        executor: DefaultMocks::times([1]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), true), res);
}

#[tokio::test]
async fn import__p2p_error() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("Some network error")));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__p2p_error_on_4_transactions() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Ok(peer_sourced_headers(Some(vec![
                empty_header(4.into()),
                empty_header(5.into()),
            ])))
        });
    let mut height = 3;
    p2p.expect_get_transactions().times(2).returning(move |_| {
        height += 1;
        if height == 4 {
            Err(anyhow::anyhow!("Some network error"))
        } else {
            Ok(Some(vec![]))
        }
    });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([2]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__p2p_error_on_5_transactions() {
    // given
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(1)
        .returning(|_| {
            Ok(peer_sourced_headers(Some(vec![
                empty_header(4.into()),
                empty_header(5.into()),
            ])))
        });
    let mut height = 3;
    p2p.expect_get_transactions().times(2).returning(move |_| {
        height += 1;
        if height == 5 {
            Err(anyhow::anyhow!("Some network error"))
        } else {
            Ok(Some(vec![]))
        }
    });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        p2p,
        consensus_port: DefaultMocks::times([2]),
        executor: DefaultMocks::times([1]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__consensus_error_on_4() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|h| {
            if **h.entity.height() == 4 {
                Err(anyhow::anyhow!("Some consensus error"))
            } else {
                Ok(true)
            }
        });
    consensus_port
        .expect_await_da_height()
        .times(1)
        .returning(|_| Ok(()));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([1]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

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
        .returning(|_| Ok(()));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([1]),
        executor: DefaultMocks::times([1]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn import__execution_error_on_header_4() {
    // given
    let mut executor = MockBlockImporterPort::default();
    executor
        .expect_execute_and_commit()
        .times(1)
        .returning(|h| {
            if **h.entity.header().height() == 4 {
                Err(anyhow::anyhow!("Some execution error"))
            } else {
                Ok(())
            }
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port: DefaultMocks::times([2]),
        p2p: DefaultMocks::times([2]),
        executor,
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), false), res);
}

#[tokio::test]
async fn import__execution_error_on_header_5() {
    // given
    let mut executor = MockBlockImporterPort::default();
    executor
        .expect_execute_and_commit()
        .times(2)
        .returning(|h| {
            if **h.entity.header().height() == 5 {
                Err(anyhow::anyhow!("Some execution error"))
            } else {
                Ok(())
            }
        });

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port: DefaultMocks::times([2]),
        p2p: DefaultMocks::times([2]),
        executor,
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(4, None), false), res);
}

#[tokio::test]
async fn signature_always_fails() {
    // given
    let mut consensus_port = MockConsensusPort::default();
    consensus_port
        .expect_check_sealed_header()
        .times(2)
        .returning(|_| Ok(false));

    let state = State::new(3, 5).into();
    let mocks = Mocks {
        consensus_port,
        p2p: DefaultMocks::times([0]),
        executor: DefaultMocks::times([0]),
    };

    // when
    let res = test_import_inner(state, mocks, None).await;

    // then
    assert_eq!((State::new(3, None), true), res);
}

#[tokio::test]
async fn import__can_work_in_two_loops() {
    // given
    let s = SharedMutex::new(State::new(3, 5));
    let state = s.clone();
    let mut p2p = MockPeerToPeerPort::default();
    p2p.expect_get_sealed_block_headers()
        .times(2)
        .returning(move |range| {
            state.apply(|s| s.observe(6));
            let headers = range.clone().map(|h| empty_header(h.into())).collect();
            Ok(peer_sourced_headers(Some(headers)))
        });
    p2p.expect_get_transactions()
        .times(3)
        .returning(move |_| Ok(Some(vec![])));
    let c = DefaultMocks::times([2]);
    let mocks = Mocks {
        consensus_port: DefaultMocks::times([3]),
        p2p,
        executor: DefaultMocks::times([3]),
    };

    // when
    let res = test_import_inner(s, mocks, Some(c)).await;

    // then
    assert_eq!((State::new(6, None), true), res);
}

async fn test_import_inner(
    state: SharedMutex<State>,
    mocks: Mocks,
    count: Option<Count>,
) -> (State, bool) {
    let notify = Arc::new(Notify::new());
    let Mocks {
        consensus_port,
        mut p2p,
        executor,
    } = mocks;
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };
    p2p.expect_report_peer().returning(|_, _| Ok(()));
    let p2p = Arc::new(p2p);

    let executor = Arc::new(executor);
    let consensus = Arc::new(consensus_port);

    let import = Import {
        state,
        notify,
        params,
        p2p,
        executor,
        consensus,
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
    PeerReportTestBuider::new()
        // When (no changes)
        // Then
        .run_with_expected_report(PeerReportReason::SuccessfulBlockImport)
        .await;
}

#[tokio::test]
async fn import__multiple_blocks_happy_path_sends_good_peer_report() {
    // Given
    PeerReportTestBuider::new()
        // When 
        .times(3)
        // Then
        .run_with_expected_report(PeerReportReason::SuccessfulBlockImport)
        .await;
}

#[tokio::test]
async fn import__missing_headers_sends_peer_report() {
    // Given
    PeerReportTestBuider::new()
        // When
        .with_get_headers(None)
        // Then
        .run_with_expected_report(PeerReportReason::MissingBlockHeaders)
        .await;
}

#[tokio::test]
async fn import__bad_block_header_sends_peer_report() {
    // Given
    PeerReportTestBuider::new()
        // When
        .with_check_sealed_header(false)
        // Then
        .run_with_expected_report(PeerReportReason::BadBlockHeader)
        .await;
}

#[tokio::test]
async fn import__missing_transactions_sends_peer_report() {
    // Given
    PeerReportTestBuider::new()
        // When
        .with_get_transactions(None)
        // Then
        .run_with_expected_report(PeerReportReason::MissingTransactions)
        .await;
}

struct PeerReportTestBuider {
    shared_peer_id: Vec<u8>,
    get_sealed_headers: Option<Option<Vec<SealedBlockHeader>>>,
    get_transactions: Option<Option<Vec<Transaction>>>,
    check_sealed_header: Option<bool>,
    block_count: u32,
    debug: bool,
}

impl PeerReportTestBuider {
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

    pub fn with_get_headers(
        mut self,
        get_headers: Option<Vec<SealedBlockHeader>>,
    ) -> Self {
        self.get_sealed_headers = Some(get_headers);
        self
    }

    pub fn with_get_transactions(
        mut self,
        get_transactions: Option<Vec<Transaction>>,
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

    pub async fn run_with_expected_report(self, expected_report: PeerReportReason) {
        if self.debug {
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        }

        let p2p = self.p2p(expected_report);
        let executor = self.executor();
        let consensus = self.consensus();

        let index = self.block_count - 1;
        let state = State::new(None, index).into();

        let notify = Arc::new(Notify::new());

        let params = Config {
            block_stream_buffer_size: 10,
            header_batch_size: 10,
        };

        let import = Import {
            state,
            notify,
            params,
            p2p,
            executor,
            consensus,
        };
        let (_tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher = shutdown.into();

        import.notify.notify_one();
        let _ = import.import(&mut watcher).await;
    }

    fn p2p(&self, expected_report: PeerReportReason) -> Arc<MockPeerToPeerPort> {
        let peer_id = self.shared_peer_id.clone();
        let mut p2p = MockPeerToPeerPort::default();

        if let Some(get_headers) = self.get_sealed_headers.clone() {
            p2p.expect_get_sealed_block_headers().returning(move |_| {
                Ok(peer_sourced_headers_peer_id(
                    get_headers.clone(),
                    peer_id.clone().into(),
                ))
            });
        } else {
            p2p.expect_get_sealed_block_headers()
                .returning(move |range| {
                    Ok(peer_sourced_headers_peer_id(
                        Some(range.clone().map(|h| empty_header(h.into())).collect()),
                        peer_id.clone().into(),
                    ))
                });
        }

        let get_transactions = self.get_transactions.clone().unwrap_or(Some(vec![]));
        p2p.expect_get_transactions()
            .returning(move |_| Ok(get_transactions.clone()));

        let peer_id = self.shared_peer_id.clone();
        p2p.expect_report_peer()
            .times(self.block_count as usize)
            .withf(move |peer, report| {
                let peer_id = peer_id.clone();
                peer.as_ref() == peer_id && report == &expected_report
            })
            .returning(|_, _| Ok(()));

        Arc::new(p2p)
    }

    fn executor(&self) -> Arc<MockBlockImporterPort> {
        let mut executor = MockBlockImporterPort::default();

        executor.expect_execute_and_commit().returning(|_| Ok(()));

        Arc::new(executor)
    }

    fn consensus(&self) -> Arc<MockConsensusPort> {
        let mut consensus_port = MockConsensusPort::default();

        consensus_port
            .expect_await_da_height()
            .returning(|_| Ok(()));

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
            .returning(|_| Ok(()));
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
                Ok(peer_sourced_headers(Some(
                    range.clone().map(|h| empty_header(h.into())).collect(),
                )))
            });
        p2p.expect_get_transactions()
            .times(t.next().unwrap())
            .returning(|_| Ok(Some(vec![])));
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
            .returning(move |_| Ok(()));
        executor
    }
}
