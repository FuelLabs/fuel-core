#![allow(non_snake_case)]

use crate::{
    import::test_helpers::empty_header,
    ports::{
        MockBlockImporterPort,
        MockConsensusPort,
        MockPeerToPeerPort,
    },
};
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
        .returning(|_| Ok(Some(Vec::new())));

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
        .returning(|_| Ok(None));

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
        .returning(|_| Ok(Some(vec![empty_header(4.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(5.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(4.into()), empty_header(5.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(4.into()), empty_header(5.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(4.into()), empty_header(5.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(4.into()), empty_header(5.into())])));
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
        .returning(|_| Ok(Some(vec![empty_header(4.into()), empty_header(5.into())])));
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
            Ok(Some(headers))
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
        p2p,
        executor,
    } = mocks;
    let params = Config {
        block_stream_buffer_size: 10,
        header_batch_size: 10,
    };
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
                Ok(Some(
                    range.clone().map(|h| empty_header(h.into())).collect(),
                ))
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
