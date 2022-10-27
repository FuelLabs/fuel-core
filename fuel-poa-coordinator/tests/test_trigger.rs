#![deny(unused_must_use)]

use anyhow::anyhow;
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    block_producer::BlockProducer,
    common::{
        consts::REG_ZERO,
        fuel_tx::TransactionBuilder,
        prelude::*,
        secrecy::{
            ExposeSecret,
            Secret,
        },
    },
    model::{
        ArcPoolTx,
        BlockHeight,
        BlockId,
        FuelBlock,
        FuelBlockConsensus,
        FuelConsensusHeader,
        PartialFuelBlock,
        PartialFuelBlockHeader,
        SecretKeyWrapper,
    },
    poa_coordinator::{
        BlockDb,
        TransactionPool,
    },
    txpool::{
        PoolTransaction,
        TxStatus,
        TxStatusBroadcast,
    },
};
use fuel_poa_coordinator::{
    Config,
    Service,
    Trigger,
};
use parking_lot::RwLock;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::Arc,
};
use tokio::{
    sync::{
        broadcast,
        mpsc,
        oneshot,
        Mutex,
    },
    task::JoinHandle,
    time::{
        self,
        Duration,
    },
};

pub struct MockBlockProducer {
    txpool_sender: MockTxPoolSender,
    database: MockDatabase,
}

impl MockBlockProducer {
    pub fn new(txpool_sender: MockTxPoolSender, database: MockDatabase) -> Self {
        Self {
            txpool_sender,
            database,
        }
    }
}

#[async_trait::async_trait]
impl BlockProducer for MockBlockProducer {
    async fn produce_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<FuelBlock> {
        let includable_txs: Vec<_> = self.txpool_sender.includable().await;

        let transactions: Vec<_> = select_transactions(includable_txs, max_gas)
            .into_iter()
            .map(|c| c.as_ref().into())
            .collect();

        self.database.inner.write().height += 1;

        Ok(PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height,
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions,
        }
        .generate(&[]))
    }

    async fn dry_run(
        &self,
        _transaction: Transaction,
        _height: Option<BlockHeight>,
        _utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>> {
        Ok(vec![])
    }
}

// TODO: The same code is in the `adapters::transaction_selector::select_transactions`. We need
//  to move transaction selection logic into `TxPool` to avoid duplication of the code in tests.
/// Select all txs that fit into the block, preferring ones with higher gas price.
fn select_transactions(
    mut includable_txs: Vec<ArcPoolTx>,
    max_gas: u64,
) -> Vec<ArcPoolTx> {
    let mut used_block_space: Word = 0;

    // Sort transactions by gas price, highest first
    includable_txs.sort_by_key(|a| Reverse(a.price()));

    // Pick as many transactions as we can fit into the block (greedy)
    includable_txs
        .into_iter()
        .filter(|tx| {
            let tx_block_space = tx.max_gas();
            if let Some(new_used_space) = used_block_space.checked_add(tx_block_space) {
                if new_used_space <= max_gas {
                    used_block_space = new_used_space;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect()
}

#[derive(Clone, Default)]
pub struct MockDatabase {
    inner: Arc<RwLock<MockDatabaseInner>>,
}

#[derive(Default)]
pub struct MockDatabaseInner {
    height: u32,
    consensus: HashMap<BlockId, FuelBlockConsensus>,
}

impl MockDatabase {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BlockDb for MockDatabase {
    fn block_height(&self) -> anyhow::Result<BlockHeight> {
        Ok(BlockHeight::from(self.inner.read().height))
    }

    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> anyhow::Result<()> {
        if self.inner.read().consensus.contains_key(&block_id) {
            Err(anyhow!("block already sealed"))
        } else {
            self.inner.write().consensus.insert(block_id, consensus);
            Ok(())
        }
    }
}

/// Txpool with manually controllable contents
pub struct MockTxPool {
    transactions: Arc<Mutex<Vec<ArcPoolTx>>>,
    broadcast_tx: broadcast::Sender<TxStatusBroadcast>,
    import_block_tx: broadcast::Sender<ImportBlockBroadcast>,
    sender: MockTxPoolSender,
    stopper: oneshot::Sender<()>,
    join: JoinHandle<()>,
    /// New blocks will be broadcast here.
    /// Messages contain the amount of transactions in the block
    block_event_rx: mpsc::Receiver<usize>,
}

impl MockTxPool {
    /// Spawn a background task for handling the messages
    fn spawn() -> (Self, broadcast::Receiver<TxStatusBroadcast>) {
        let transactions = Arc::new(Mutex::new(Vec::<ArcPoolTx>::new()));

        let (block_event_tx, block_event_rx) = mpsc::channel(16);

        let (stopper_tx, mut stopper_rx) = oneshot::channel();
        let (txpool_tx, mut txpool_rx) = mpsc::channel(16);
        let (broadcast_tx, broadcast_rx) = broadcast::channel(16);
        let (import_block_tx, mut import_block_rx) = broadcast::channel(16);

        let txs = transactions.clone();
        let join = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stopper_rx => {
                        break;
                    },
                    msg = txpool_rx.recv() => {
                        match msg.expect("Closed unexpectedly") {
                            MockTxPoolMsg::Includable(response) => {
                                let resp = txs.lock().await.clone();
                                response.send(resp).unwrap();
                            },
                            MockTxPoolMsg::ConsumableGas(response) => {
                                let t = txs.lock().await.clone();
                                let resp = t.into_iter().map(|t| t.limit()).sum();
                                response.send(resp).unwrap();
                            }
                        }
                    },
                    msg = import_block_rx.recv() => {
                        match msg.expect("Closed unexpectedly") {
                            ImportBlockBroadcast::PendingFuelBlockImported { block } => {
                                let mut g = txs.lock().await;
                                for tx in block.transactions() {
                                    let i = g.iter().position(|t| t.id() == tx.id()).unwrap();
                                    g.swap_remove(i);
                                }
                                block_event_tx.send(block.transactions().len()).await.unwrap();
                            },
                            _ => todo!("This block import type is not mocked yet"),
                        }
                    },
                }
            }
        });

        (
            Self {
                transactions,
                broadcast_tx,
                import_block_tx,
                sender: MockTxPoolSender(txpool_tx),
                stopper: stopper_tx,
                join,
                block_event_rx,
            },
            broadcast_rx,
        )
    }

    fn sender(&self) -> MockTxPoolSender {
        self.sender.clone()
    }

    async fn add_tx(&mut self, tx: ArcPoolTx) {
        self.transactions.lock().await.push(tx.clone());
        self.broadcast_tx
            .send(TxStatusBroadcast {
                tx,
                status: TxStatus::Submitted,
            })
            .unwrap();
    }

    fn check_block_produced(&mut self) -> Result<usize, mpsc::error::TryRecvError> {
        self.block_event_rx.try_recv()
    }

    async fn wait_block_produced(&mut self) -> usize {
        self.block_event_rx.recv().await.expect("Disconnected")
    }

    async fn stop(self) -> anyhow::Result<()> {
        self.stopper.send(()).expect("Stopping failed");
        self.join.await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum MockTxPoolMsg {
    ConsumableGas(oneshot::Sender<u64>),
    Includable(oneshot::Sender<Vec<ArcPoolTx>>),
}

#[derive(Clone)]
pub struct MockTxPoolSender(mpsc::Sender<MockTxPoolMsg>);

impl MockTxPoolSender {
    async fn includable(&self) -> Vec<ArcPoolTx> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(MockTxPoolMsg::Includable(tx))
            .await
            .expect("Send error");
        rx.await.expect("MockTxPool panicked in includable query")
    }
}

fn test_signing_key() -> Secret<SecretKeyWrapper> {
    let mut rng = StdRng::seed_from_u64(0);
    let secret_key = SecretKey::random(&mut rng);
    Secret::new(secret_key.into())
}

#[async_trait::async_trait]
impl TransactionPool for MockTxPoolSender {
    async fn total_consumable_gas(&self) -> anyhow::Result<u64> {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(MockTxPoolMsg::ConsumableGas(tx))
            .await
            .expect("Send error");
        Ok(rx
            .await
            .expect("MockTxPool panicked in total_consumable_gas query"))
    }
}

#[tokio::test(start_paused = true)] // Run with time paused, start/stop must still work
async fn clean_startup_shutdown_each_trigger() -> anyhow::Result<()> {
    for trigger in [
        Trigger::Never,
        Trigger::Instant,
        Trigger::Interval {
            block_time: Duration::new(1, 0),
        },
        Trigger::Hybrid {
            min_block_time: Duration::new(1, 0),
            max_tx_idle_time: Duration::new(1, 0),
            max_block_time: Duration::new(1, 0),
        },
    ] {
        let db = MockDatabase::new();

        let service = Service::new(&Config {
            trigger,
            block_gas_limit: 100_000,
            signing_key: Some(test_signing_key()),
        });

        let (txpool, broadcast_rx) = MockTxPool::spawn();

        service
            .start(
                broadcast_rx,
                txpool.sender(),
                txpool.import_block_tx.clone(),
                Arc::new(MockBlockProducer::new(txpool.sender(), db.clone())),
                db,
            )
            .await;

        let handle = service.stop().await.expect("Get join handle");

        handle.await?;
    }

    Ok(())
}

struct CoinInfo {
    index: u8,
    id: Bytes32,
    secret_key: SecretKey,
}

impl CoinInfo {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.id, self.index)
    }
}

fn _make_tx(coin: &CoinInfo, gas_price: u64, gas_limit: u64) -> PoolTransaction {
    TransactionBuilder::script(vec![Opcode::RET(REG_ZERO)].into_iter().collect(), vec![])
        .gas_price(gas_price)
        .gas_limit(gas_limit)
        .add_unsigned_coin_input(
            coin.secret_key,
            coin.utxo_id(),
            1_000_000_000,
            AssetId::zeroed(),
            Default::default(),
            0,
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: AssetId::zeroed(),
        })
        .finalize_checked_basic(Default::default(), &Default::default())
        .into()
}

fn make_tx() -> PoolTransaction {
    let mut rng = StdRng::seed_from_u64(1234u64);
    _make_tx(
        &CoinInfo {
            index: 0,
            id: rng.gen(),
            secret_key: SecretKey::random(&mut rng),
        },
        1,
        10_000,
    )
}

#[tokio::test(start_paused = true)]
async fn never_trigger_never_produces_blocks() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Never,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db,
        )
        .await;

    // Submit some txs
    for _ in 0..10 {
        txpool.add_tx(Arc::new(make_tx())).await;
    }

    // Make sure enough time passes for the block to be produced
    time::sleep(Duration::new(10, 0)).await;

    // Make sure no blocks are produced
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn instant_trigger_produces_block_instantly() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Instant,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());

    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db.clone(),
        )
        .await;

    // Submit tx
    txpool.add_tx(Arc::new(make_tx())).await;

    // Make sure it's produced
    assert_eq!(txpool.wait_block_produced().await, 1);

    // Checked that it's sealed and signature is valid
    {
        let db_lock = db.inner.read();
        let (id, consensus) = db_lock
            .consensus
            .iter()
            .next()
            .expect("expected sealed block info");
        match consensus {
            FuelBlockConsensus::PoA(poa) => {
                // verify against public key from test config
                let pk = test_signing_key().expose_secret().public_key();

                let message = id.into_message();

                poa.signature
                    .verify(&pk, &message)
                    .expect("expected signature to be valid");
            } //_ => panic!("invalid sealed data"),
        }
    }

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_produces_blocks_periodically() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db,
        )
        .await;

    // Make sure no blocks are produced yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a single block is produced, and a bit more
    time::sleep(Duration::new(3, 0)).await;

    // Make sure the empty block is actually produced
    assert_eq!(txpool.check_block_produced(), Ok(0));

    // Submit tx
    txpool.add_tx(Arc::new(make_tx())).await;

    // Make sure no blocks are produced before next interval
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure it's produced
    assert_eq!(txpool.check_block_produced(), Ok(1));

    // Submit two tx
    for _ in 0..2 {
        txpool.add_tx(Arc::new(make_tx())).await;
    }

    time::sleep(Duration::from_millis(1)).await;

    // Make sure blocks are not produced before the block time is used
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert_eq!(txpool.check_block_produced(), Ok(2));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert_eq!(txpool.check_block_produced(), Ok(0));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_doesnt_react_to_full_txpool() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db,
        )
        .await;

    // Fill txpool completely
    for _ in 0..1_000 {
        txpool.add_tx(Arc::new(make_tx())).await;
        tokio::spawn(async {}).await.unwrap(); // Process messages so the channel doesn't lag
    }

    // Make sure blocks are not produced before the block time has elapsed
    time::sleep(Duration::new(1, 0)).await;
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure only one block per round is produced
    for _ in 0..5 {
        time::sleep(Duration::new(2, 0)).await;
        assert!(txpool.check_block_produced().is_ok());
        assert_eq!(
            txpool.check_block_produced(),
            Err(mpsc::error::TryRecvError::Empty)
        );
    }

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(2, 0),
            max_tx_idle_time: Duration::new(3, 0),
            max_block_time: Duration::new(10, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db,
        )
        .await;

    // Make sure no blocks are produced yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure no blocks are produced when txpool is empty and max_block_time is not exceeded
    time::sleep(Duration::new(9, 0)).await;

    // Make sure the empty block is actually produced
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Submit tx
    txpool.add_tx(Arc::new(make_tx())).await;

    // Make sure no block is produced immediately, as none of the timers has expired yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a single block is produced after idle time
    time::sleep(Duration::new(4, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(1));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure the empty block is produced after max_block_time
    time::sleep(Duration::new(10, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(0));

    // Submit two tx
    for _ in 0..2 {
        txpool.add_tx(Arc::new(make_tx())).await;
    }

    // Wait for both max_tx_idle_time and min_block_time to pass, and see that the block is produced
    time::sleep(Duration::new(4, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(2));

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_reacts_correctly_to_full_txpool() -> anyhow::Result<()> {
    let db = MockDatabase::new();

    let service = Service::new(&Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(2, 0),
            max_tx_idle_time: Duration::new(3, 0),
            max_block_time: Duration::new(10, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
    });

    let (mut txpool, broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let producer = Arc::new(producer);
    service
        .start(
            broadcast_rx,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            producer.clone(),
            db,
        )
        .await;

    // Fill txpool completely
    for _ in 0..100 {
        txpool.add_tx(Arc::new(make_tx())).await;
        tokio::task::yield_now().await; // Process messages so the channel doesn't lag
    }

    // Make sure blocks are not produced before the min block time has elapsed
    time::sleep(Duration::new(1, 0)).await;
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure only blocks are produced immediately after min_block_time, but no sooner
    for _ in 0..5 {
        time::sleep(Duration::new(2, 0)).await;
        tokio::task::yield_now().await;
        assert!(txpool.check_block_produced().is_ok());
        assert_eq!(
            txpool.check_block_produced(),
            Err(mpsc::error::TryRecvError::Empty)
        );
    }

    // Stop
    let handle = service.stop().await.expect("Get join handle");
    txpool.stop().await?;
    handle.await?;

    Ok(())
}
