use fuel_core_types::blockchain::{
    consensus::Consensus,
    header::BlockHeader,
};

use crate::ports::{
    MockBlockImporterPort,
    MockConsensusPort,
    MockPeerToPeerPort,
};
use test_case::test_case;

use super::*;

#[test_case(State::new(None, 5), Mocks::times([6]) => (State::new(5, None), true) ; "executes 5")]
#[test_case(State::new(3, 5), Mocks::times([2]) => (State::new(5, None), true) ; "executes 3 to 5")]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(1)
            .returning(|_| Ok(false));
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "Signature always fails"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(2)
            .returning(|h| Ok(**h.entity.height() != 5));
        consensus_port.expect_await_da_height()
            .times(1)
            .returning(|_| Ok(()));
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 1]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), true) ; "Signature fails on header 5 only"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(1)
            .returning(|h| Ok(**h.entity.height() != 4));
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "Signature fails on header 4 only"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(1)
            .returning(|_| Ok(None));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "Header not found"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok((*h != 5).then(|| empty_header(h))));
        p2p.expect_get_transactions()
            .times(1)
            .returning(|_| Ok(Some(vec![])));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), true) ; "Header 5 not found"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(1)
            .returning(|h| Ok((*h != 4).then(|| empty_header(h))));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "Header 4 not found"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        p2p.expect_get_transactions()
            .times(1)
            .returning(|_| Ok(None));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "transactions not found"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        let mut count = 0;
        p2p.expect_get_transactions()
            .times(1)
            .returning(move|_| {
                count += 1;
                if count > 1 {
                    Ok(Some(vec![]))
                } else {
                    Ok(None)
                }
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), true) ; "transactions not found for header 4"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        let mut count = 0;
        p2p.expect_get_transactions()
            .times(2)
            .returning(move|_| {
                count += 1;
                if count > 1 {
                    Ok(None)
                } else {
                    Ok(Some(vec![]))
                }
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([2]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), true) ; "transactions not found for header 5"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Some network error")));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false); "p2p error"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(1)
            .returning(|h| if *h == 4 {
                Err(anyhow::anyhow!("Some network error"))
            } else {
                Ok(Some(empty_header(h)))
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false); "header 4 p2p error"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| if *h == 5 {
                Err(anyhow::anyhow!("Some network error"))
            } else {
                Ok(Some(empty_header(h)))
            });
        p2p.expect_get_transactions()
            .times(1)
            .returning(|_| Ok(Some(vec![])));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), false); "header 5 p2p error"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        p2p.expect_get_transactions()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Some network error")));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false); "p2p error on transactions"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        let mut count = 0;
        p2p.expect_get_transactions()
            .times(1)
            .returning(move|_| {
                count += 1;
                if count > 1 {
                    Ok(Some(vec![]))
                } else {
                    Err(anyhow::anyhow!("Some network error"))
                }
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([1]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false); "p2p error on 4 transactions"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(2)
            .returning(|h| Ok(Some(empty_header(h))));
        let mut count = 0;
        p2p.expect_get_transactions()
            .times(2)
            .returning(move|_| {
                count += 1;
                if count > 1 {
                    Err(anyhow::anyhow!("Some network error"))
                } else {
                    Ok(Some(vec![]))
                }
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([2]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), false); "p2p error on 5 transactions"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Some consensus error")));
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false) ; "consensus error"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(1)
            .returning(|h| if **h.entity.height() == 4 {
                Err(anyhow::anyhow!("Some consensus error"))
            } else {
                Ok(true)
            });
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 0]),
            executor: DefaultMocks::times([0])
        }
    }
    => (State::new(3, None), false) ; "consensus error on 4"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut consensus_port = MockConsensusPort::default();
        consensus_port.expect_check_sealed_header()
            .times(2)
            .returning(|h| if **h.entity.height() == 5 {
                Err(anyhow::anyhow!("Some consensus error"))
            } else {
                Ok(true)
            });
        consensus_port.expect_await_da_height()
            .times(1)
            .returning(|_| Ok(()));
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 1]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), false) ; "consensus error on 5"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut executor = MockBlockImporterPort::default();
        executor
            .expect_execute_and_commit()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Some execution error")));
        Mocks{
            consensus_port: DefaultMocks::times([1]),
            p2p: DefaultMocks::times([2, 1]),
            executor,
        }
    }
    => (State::new(3, None), false) ; "execution error"
)]
#[test_case(
    State::new(3, 5),
    {
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
        Mocks{
            consensus_port: DefaultMocks::times([1]),
            p2p: DefaultMocks::times([2, 1]),
            executor,
        }
    }
    => (State::new(3, None), false) ; "execution error on header 4"
)]
#[test_case(
    State::new(3, 5),
    {
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
        Mocks{
            consensus_port: DefaultMocks::times([2]),
            p2p: DefaultMocks::times([2, 2]),
            executor,
        }
    }
    => (State::new(4, None), false) ; "execution error on header 5"
)]
#[tokio::test]
async fn test_import(state: State, mocks: Mocks) -> (State, bool) {
    let state = SharedMutex::new(state);
    test_import_inner(state, mocks, None).await
}

#[test_case(
    {
        let s = SharedMutex::new(State::new(3, 5));
        let state = s.clone();
        let mut p2p = MockPeerToPeerPort::default();
        p2p.expect_get_sealed_block_header()
            .times(3)
            .returning(move |h| {
                state.apply(|s| s.observe(6));
                Ok(Some(empty_header(h)))
            });
        p2p.expect_get_transactions()
            .times(3)
            .returning(move|_| Ok(Some(vec![])));
        let c = DefaultMocks::times([2]);
        (s, c, Mocks{
            consensus_port: DefaultMocks::times([3]),
            p2p,
            executor: DefaultMocks::times([3]),
        })
    }
    => (State::new(6, None), true) ; "Loop 1 with headers 4, 5. Loop 2 with header 6"
)]
#[tokio::test]
async fn test_import_loop(
    (state, count, mocks): (SharedMutex<State>, Count, Mocks),
) -> (State, bool) {
    test_import_inner(state, mocks, Some(count)).await
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
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
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
    let r = match count {
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
    let s = import.state.apply(|s| s.clone());
    (s, r)
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

        p2p.expect_get_sealed_block_header()
            .times(t.next().unwrap())
            .returning(|h| Ok(Some(empty_header(h))));
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

pub(crate) fn empty_header(h: BlockHeight) -> SourcePeer<SealedBlockHeader> {
    let mut header = BlockHeader::default();
    header.consensus.height = h;
    let transaction_tree =
        fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
    header.application.generated.transactions_root = transaction_tree.root().into();

    let consensus = Consensus::default();
    let sealed = Sealed {
        entity: header,
        consensus,
    };
    SourcePeer {
        peer_id: vec![].into(),
        data: sealed,
    }
}
