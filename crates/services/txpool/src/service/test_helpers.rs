use super::*;
use crate::{
    ports::GossipValidity,
    MockDb,
};
use fuel_core_interfaces::txpool::Sender;
use fuel_core_types::{
    entities::coin::Coin,
    fuel_crypto::rand::{
        rngs::StdRng,
        SeedableRng,
    },
    fuel_tx::{
        Input,
        Transaction,
        TransactionBuilder,
        Word,
    },
};
use futures::stream::BoxStream;
use mockall::mock;
use std::{
    any::Any,
    cell::RefCell,
};
use tokio::sync::{
    mpsc::Receiver,
    oneshot,
};

pub struct TestContext {
    pub(crate) service: Service,
    mock_db: Box<MockDb>,
    _drop_resources: Vec<Box<dyn Any>>,
    rng: RefCell<StdRng>,
    pub(crate) p2p: Arc<MockP2PAdapter>,
}

impl TestContext {
    pub async fn new() -> Self {
        TestContextBuilder::new().build().await
    }

    pub fn service(&self) -> &Service {
        &self.service
    }

    pub fn setup_script_tx(&self, gas_price: Word) -> Transaction {
        let (_, gas_coin) = self.setup_coin();
        TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price)
            .add_input(gas_coin)
            .finalize_as_transaction()
    }

    pub fn setup_coin(&self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng.borrow_mut(), Some(&self.mock_db))
    }

    pub async fn stop(&self) {
        self.service.stop().await.unwrap().await.unwrap();
    }
}

pub struct TestContextBuilder {
    mock_db: MockDb,
    rng: StdRng,
    pub(crate) p2p: Option<MockP2PAdapter>,
}

impl TestContextBuilder {
    pub fn new() -> Self {
        Self {
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(10),
            p2p: None,
        }
    }

    pub fn with_p2p(&mut self, p2p: MockP2PAdapter) {
        self.p2p = Some(p2p)
    }

    pub fn setup_script_tx(&mut self, gas_price: Word) -> Transaction {
        let (_, gas_coin) = self.setup_coin();
        TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price)
            .add_input(gas_coin)
            .finalize_as_transaction()
    }

    pub fn setup_coin(&mut self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng, Some(&self.mock_db))
    }

    pub async fn build(self) -> TestContext {
        let rng = RefCell::new(self.rng);
        let config = Config::default();
        let mock_db = self.mock_db;
        let (block_tx, block_rx) = broadcast::channel(10);
        let status_tx = TxStatusChange::new(100);
        let (txpool_tx, txpool_rx) = Sender::channel(100);

        let p2p = Arc::new(self.p2p.unwrap_or_else(MockP2PAdapter::default));

        let mut builder = ServiceBuilder::new();
        builder
            .config(config)
            .db(Box::new(mock_db.clone()))
            .import_block_event(block_rx)
            .tx_status_sender(status_tx)
            .txpool_sender(txpool_tx)
            .txpool_receiver(txpool_rx)
            .p2p_port(p2p.clone());

        let service = builder.build().unwrap();
        service.start().await.unwrap();

        // resources to keep alive for the during of the test context
        let drop_resources: Vec<Box<dyn Any>> = vec![Box::new(block_tx)];

        TestContext {
            service,
            mock_db: Box::new(mock_db),
            _drop_resources: drop_resources,
            rng,
            p2p,
        }
    }
}

type GossipedTransaction = GossipData<Transaction>;

pub struct MockP2PAdapter {
    gossip_txs: Mutex<Vec<Transaction>>,
    broadcasted_txs: (
        broadcast::Sender<Arc<Transaction>>,
        broadcast::Receiver<Arc<Transaction>>,
    ),
}

impl Default for MockP2PAdapter {
    fn default() -> Self {
        MockP2PAdapter::new(vec![])
    }
}

impl MockP2PAdapter {
    pub fn new(gossip_txs: Vec<Transaction>) -> Self {
        Self {
            gossip_txs: Mutex::new(gossip_txs),
            broadcasted_txs: broadcast::channel(100),
        }
    }

    pub fn broadcast_subscribe(&self) -> broadcast::Receiver<Arc<Transaction>> {
        self.broadcasted_txs.0.subscribe()
    }
}

#[async_trait::async_trait]
impl PeerToPeer for MockP2PAdapter {
    type GossipedTransaction = GossipedTransaction;
    // Gossip broadcast a transaction inserted via API.
    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.broadcasted_txs.0.send(transaction).unwrap();
        Ok(())
    }

    // Await the next transaction from network gossip (similar to stream.next()).
    async fn next_gossiped_transaction(&self) -> GossipedTransaction {
        let mut gossip_txs = self.gossip_txs.lock().await;
        if let Some(tx) = gossip_txs.pop() {
            // TODO: provide peer id and message id for testing validity reporting
            GossipedTransaction::new(tx, vec![], vec![])
        } else {
            // await indefinitely
            let (_tx, rx) = oneshot::channel::<()>();
            rx.await.unwrap();
            unreachable!()
        }
    }

    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        _message: &GossipedTransaction,
        _validity: GossipValidity,
    ) {
        // do nothing
    }
}
