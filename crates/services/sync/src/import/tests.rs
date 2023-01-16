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
use tokio::sync::oneshot;

use crate::ports::{
    MockConsensusPort,
    MockExecutorPort,
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
            count: DefaultMocks::times([1]),
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
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 1]),
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([1]),
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
            .times(2)
            .returning(|_| Err(anyhow::anyhow!("Some network error")));
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            count: DefaultMocks::times([1]),
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
            .times(2)
            .returning(|h| if *h == 4 {
                Err(anyhow::anyhow!("Some network error"))
            } else {
                Ok(Some(empty_header(h)))
            });
        Mocks{
            p2p,
            consensus_port: DefaultMocks::times([0]),
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([0]),
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
            count: DefaultMocks::times([0]),
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
            count: DefaultMocks::times([0]),
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
            count: DefaultMocks::times([1]),
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
            count: DefaultMocks::times([0]),
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
            count: DefaultMocks::times([0]),
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
        Mocks{
            consensus_port,
            p2p: DefaultMocks::times([2, 1]),
            count: DefaultMocks::times([1]),
            executor: DefaultMocks::times([1])
        }
    }
    => (State::new(4, None), false) ; "consensus error on 5"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut executor = MockExecutorPort::default();
        executor
            .expect_execute_and_commit()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("Some execution error")));
        Mocks{
            consensus_port: DefaultMocks::times([1]),
            p2p: DefaultMocks::times([2, 1]),
            count: DefaultMocks::times([1]),
            executor,
        }
    }
    => (State::new(3, None), false) ; "execution error"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut executor = MockExecutorPort::default();
        executor
            .expect_execute_and_commit()
            .times(1)
            .returning(|h| {
                if **h.entity.header().height() == 4 {
                    Err(anyhow::anyhow!("Some execution error"))
                } else {
                    Ok(ExecutionResult {
                        block: Block::default(),
                        skipped_transactions: vec![],
                        tx_status: vec![],
                    })
                }
            });
        Mocks{
            consensus_port: DefaultMocks::times([1]),
            p2p: DefaultMocks::times([2, 1]),
            count: DefaultMocks::times([0]),
            executor,
        }
    }
    => (State::new(3, None), false) ; "execution error on header 4"
)]
#[test_case(
    State::new(3, 5),
    {
        let mut executor = MockExecutorPort::default();
        executor
            .expect_execute_and_commit()
            .times(2)
            .returning(|h| {
                if **h.entity.header().height() == 5 {
                    Err(anyhow::anyhow!("Some execution error"))
                } else {
                    Ok(ExecutionResult {
                        block: Block::default(),
                        skipped_transactions: vec![],
                        tx_status: vec![],
                    })
                }
            });
        Mocks{
            consensus_port: DefaultMocks::times([2]),
            p2p: DefaultMocks::times([2, 2]),
            count: DefaultMocks::times([1]),
            executor,
        }
    }
    => (State::new(4, None), false) ; "execution error on header 5"
)]
#[tokio::test]
async fn test_import(state: State, mocks: Mocks) -> (State, bool) {
    let Mocks {
        consensus_port,
        p2p,
        executor,
        count: Count(rx, loop_callback),
    } = mocks;
    let state = SharedMutex::new(state);
    let notify = Arc::new(Notify::new());
    let params = Params {
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
    };
    let p2p = Arc::new(p2p);

    let executor = Arc::new(executor);
    let consensus = Arc::new(consensus_port);
    let ports = Ports {
        p2p,
        executor,
        consensus,
    };
    let mut ks = KillSwitch::new();
    let jh = tokio::spawn(import(
        state.clone(),
        notify,
        params,
        ports,
        ks.handle(),
        loop_callback,
    ));
    if let Ok(r) = tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        r.ok();
    }
    ks.kill_all();
    let r = jh.await.unwrap().is_ok();
    let s = state.apply(|s| s.clone());
    (s, r)
}

struct Mocks {
    consensus_port: MockConsensusPort,
    p2p: MockPeerToPeerPort,
    executor: MockExecutorPort,
    count: Count,
}

struct Count(
    oneshot::Receiver<()>,
    Box<dyn FnMut() + Send + Sync + 'static>,
);

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
            count: Count::times([1]),
        }
    }
}

impl DefaultMocks for Count {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        let (rx, f) = count(t.into_iter().next().unwrap());
        Self(rx, Box::new(f))
    }
}

impl DefaultMocks for MockConsensusPort {
    fn times<T>(t: T) -> Self
    where
        T: IntoIterator<Item = usize> + Clone,
        <T as IntoIterator>::IntoIter: Clone,
    {
        let mut consensus_port = MockConsensusPort::new();
        consensus_port
            .expect_check_sealed_header()
            .times(t.into_iter().next().unwrap())
            .returning(|_| Ok(true));
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

impl DefaultMocks for MockExecutorPort {
    fn times<T: IntoIterator<Item = usize> + Clone>(t: T) -> Self {
        let mut executor = MockExecutorPort::default();
        let t = t.into_iter().next().unwrap();

        executor
            .expect_execute_and_commit()
            .times(t)
            .returning(move |_| {
                Ok(ExecutionResult {
                    block: Block::default(),
                    skipped_transactions: vec![],
                    tx_status: vec![],
                })
            });
        executor
    }
}

fn count(t: usize) -> (oneshot::Receiver<()>, impl FnMut()) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    let mut count = 0;
    (rx, move || {
        count += 1;
        if count == t {
            tx.take().unwrap().send(()).unwrap();
        }
    })
}

fn empty_header(h: BlockHeight) -> SourcePeer<SealedBlockHeader> {
    let mut header = BlockHeader::default();
    header.consensus.height = h;

    let consensus = Consensus::default();
    let sealed = Sealed {
        entity: header,
        consensus,
    };
    let peer_id = PeerId {};
    SourcePeer {
        peer_id,
        data: sealed,
    }
}
