use super::*;
use crate::{
    ports::BlockImport,
    MockDb,
};
use fuel_core_types::{
    blockchain::SealedBlock,
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
use std::cell::RefCell;

type GossipedTransaction = GossipData<Transaction>;

pub struct TestContext {
    pub(crate) service: Service,
    mock_db: Box<MockDb>,
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
            &'_self mut self,
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
    pub fn new_with_txs(mut txs: Vec<Transaction>) -> Self {
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
        p2p.expect_broadcast_transaction()
            .returning(move |_| Ok(()));
        p2p
    }
}

mockall::mock! {
    pub Importer {}

    #[async_trait::async_trait]
    impl BlockImport for Importer {
        fn next_block<'_self, 'a>(&'_self mut self) -> BoxFuture<'a, SealedBlock>
        where
            '_self: 'a,
            Self: Sync + 'a;

    }
}

impl MockImporter {
    fn with_blocks(mut blocks: Vec<SealedBlock>) -> Self {
        let mut importer = MockImporter::default();
        importer.expect_next_block().returning(move || {
            let block = blocks.pop();
            Box::pin(async move {
                if let Some(block) = block {
                    block
                } else {
                    core::future::pending::<SealedBlock>().await
                }
            })
        });
        importer
    }
}

pub struct TestContextBuilder {
    mock_db: MockDb,
    rng: StdRng,
    p2p: Option<MockP2P>,
    importer: Option<MockImporter>,
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
            importer: None,
        }
    }

    pub fn with_importer(&mut self, importer: MockImporter) {
        self.importer = Some(importer)
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
        let status_tx = TxStatusChange::new(100);

        let p2p = Box::new(self.p2p.unwrap_or_else(|| MockP2P::new_with_txs(vec![])));
        let importer = Box::new(
            self.importer
                .unwrap_or_else(|| MockImporter::with_blocks(vec![])),
        );

        let mut builder = ServiceBuilder::new();
        builder
            .config(config)
            .db(Arc::new(mock_db.clone()))
            .importer(importer)
            .tx_status_sender(status_tx)
            .p2p_port(p2p);

        let service = builder.build().unwrap();
        service.start().await.unwrap();

        TestContext {
            service,
            mock_db: Box::new(mock_db),
            rng,
        }
    }
}
