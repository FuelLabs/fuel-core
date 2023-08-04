use criterion::{
    async_executor::AsyncExecutor,
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core_services::{
    stream::BoxStream,
    SharedMutex,
    StateWatcher,
};
use tokio::runtime::Runtime;

use fuel_core_storage::InterpreterStorage;
use fuel_core_sync::{
    import::{
        Config,
        Import,
    },
    ports::{
        BlockImporterPort,
        ConsensusPort,
        MockBlockImporterPort,
        MockConsensusPort,
        MockPeerToPeerPort,
        PeerToPeerPort,
    },
    state::State,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Sealed,
        },
        header::{
            BlockHeader,
            GeneratedConsensusFields,
        },
        primitives::{
            BlockId,
            DaBlockHeight,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::{
        Bytes32,
        Transaction,
    },
    fuel_types::{
        BlockHeight,
        ContractId,
    },
    services::p2p::SourcePeer,
};
use rand::{
    rngs::StdRng,
    thread_rng,
    Rng,
    SeedableRng,
};
use std::{
    iter,
    sync::{
        Arc,
        Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Default)]
struct Input {
    headers: Duration,
    consensus: Duration,
    transactions: Duration,
    executes: Duration,
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

struct PressurePeerToPeerPort(MockPeerToPeerPort, [Duration; 2]);
struct PressureBlockImporterPort(MockBlockImporterPort, Duration);
struct PressureConsensusPort(MockConsensusPort, Duration);

#[async_trait::async_trait]
impl PeerToPeerPort for PressurePeerToPeerPort {
    fn height_stream(&self) -> BoxStream<BlockHeight> {
        self.0.height_stream()
    }

    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>> {
        tokio::time::sleep(self.1[0]).await;
        self.0.get_sealed_block_header(height).await
    }

    async fn get_transactions(
        &self,
        block_id: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        tokio::time::sleep(self.1[1]).await;
        self.0.get_transactions(block_id).await
    }
}

#[async_trait::async_trait]
impl BlockImporterPort for PressureBlockImporterPort {
    fn committed_height_stream(&self) -> BoxStream<BlockHeight> {
        self.0.committed_height_stream()
    }

    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()> {
        let timeout = self.1;
        tokio::task::spawn_blocking(move || {
            std::thread::sleep(timeout);
        })
        .await
        .unwrap();
        self.0.execute_and_commit(block).await
    }
}

#[async_trait::async_trait]
impl ConsensusPort for PressureConsensusPort {
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool> {
        self.0.check_sealed_header(header)
    }

    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()> {
        tokio::time::sleep(self.1).await;
        self.0.await_da_height(da_height).await
    }
}

impl PressurePeerToPeerPort {
    fn new(delays: [Duration; 2]) -> Self {
        let mut mock = MockPeerToPeerPort::default();
        mock.expect_get_sealed_block_header()
            .returning(|h| Ok(Some(empty_header(h))));
        mock.expect_get_transactions()
            .returning(|_| Ok(Some(vec![])));
        Self(mock, delays)
    }
}

impl PressureBlockImporterPort {
    fn new(delays: Duration) -> Self {
        let mut mock = MockBlockImporterPort::default();
        mock.expect_execute_and_commit().returning(move |_| Ok(()));
        Self(mock, delays)
    }
}

impl PressureConsensusPort {
    fn new(delays: Duration) -> Self {
        let mut mock = MockConsensusPort::default();
        mock.expect_await_da_height().returning(|_| Ok(()));
        mock.expect_check_sealed_header().returning(|_| Ok(true));
        Self(mock, delays)
    }
}

async fn test() {
    let input = Input {
        headers: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
        ..Default::default()
    };
    let params = Config {
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
    };
    let state = State::new(None, 50);
    let state = SharedMutex::new(state);

    let p2p = Arc::new(PressurePeerToPeerPort::new([
        input.headers,
        input.transactions,
    ]));

    let executor = Arc::new(PressureBlockImporterPort::new(input.executes));
    let consensus = Arc::new(PressureConsensusPort::new(input.consensus));
    let notify = Arc::new(Notify::new());

    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();

    let import = Import::new(state, notify, params, p2p, executor, consensus);

    import.notify.notify_one();
    import.import(&mut watcher).await.unwrap();
}

fn import_one(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("import");
    group.bench_function("import one", |b| b.to_async(&rt).iter(|| test()));
}

criterion_group!(benches, import_one);
criterion_main!(benches);
