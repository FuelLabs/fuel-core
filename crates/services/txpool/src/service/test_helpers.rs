use super::*;
use crate::MockDb;
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
    services::p2p::GossipsubMessageAcceptance,
};
use std::{
    any::Any,
    cell::RefCell,
};

type GossipedTransaction = GossipData<Transaction>;

pub struct TestContext {
    pub(crate) service: Service,
    mock_db: Box<MockDb>,
    _drop_resources: Vec<Box<dyn Any>>,
    rng: RefCell<StdRng>,
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

pub type BoxFuture<'a, T> =
    core::pin::Pin<Box<dyn core::future::Future<Output = T> + Send + 'a>>;

mockall::mock! {
    pub P2P {}

    #[async_trait::async_trait]
    impl PeerToPeer for P2P {
        type GossipedTransaction = GossipedTransaction;

        async fn broadcast_transaction(
            &self,
            transaction: Arc<Transaction>,
        ) -> anyhow::Result<()>;

        fn next_gossiped_transaction<'_self, 'a>(
            &'_self self,
        ) -> BoxFuture<'a, GossipedTransaction>
        where
            '_self: 'a,
            Self: Sync + 'a;

        async fn notify_gossip_transaction_validity(
            &self,
            message: &GossipedTransaction,
            validity: GossipsubMessageAcceptance,
        );
    }
}

impl MockP2P {
    pub fn with_txs(mut txs: Vec<Transaction>) -> Self {
        let mut p2p = MockP2P::default();
        p2p.expect_next_gossiped_transaction().returning(move || {
            let tx = txs.pop();
            Box::pin(async move {
                if let Some(tx) = tx {
                    GossipData::new(tx, vec![], vec![])
                } else {
                    core::future::pending::<GossipData<Transaction>>().await
                }
            })
        });
        p2p
    }
}

pub struct TestContextBuilder {
    mock_db: MockDb,
    rng: StdRng,
    p2p: Option<MockP2P>,
}

impl Default for TestContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestContextBuilder {
    pub fn new() -> Self {
        Self {
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(10),
            p2p: None,
        }
    }

    pub fn with_p2p(&mut self, p2p: MockP2P) {
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

        let p2p = Arc::new(self.p2p.unwrap_or_else(MockP2P::default));

        let mut builder = ServiceBuilder::new();
        builder
            .config(config)
            .db(Box::new(mock_db.clone()))
            .importer(block_rx)
            .tx_status_sender(status_tx)
            .txpool_sender(txpool_tx)
            .txpool_receiver(txpool_rx)
            .p2p_port(p2p);

        let service = builder.build().unwrap();
        service.start().await.unwrap();

        // resources to keep alive for the during of the test context
        let drop_resources: Vec<Box<dyn Any>> = vec![Box::new(block_tx)];

        TestContext {
            service,
            mock_db: Box::new(mock_db),
            _drop_resources: drop_resources,
            rng,
        }
    }
}
