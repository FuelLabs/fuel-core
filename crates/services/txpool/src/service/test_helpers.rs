use super::*;
use crate::{
    ports::BlockImporter,
    MockDb,
};
use fuel_core_services::{
    stream::BoxStream,
    Service as ServiceTrait,
};
use fuel_core_types::{
    blockchain::SealedBlock,
    entities::coins::coin::Coin,
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
    pub(crate) service: Service<MockP2P, MockDb>,
    mock_db: MockDb,
    rng: RefCell<StdRng>,
}

impl TestContext {
    pub async fn new() -> Self {
        TestContextBuilder::new().build_and_start().await
    }

    pub fn service(&self) -> &Service<MockP2P, MockDb> {
        &self.service
    }

    pub fn setup_script_tx(&self, gas_price: Word) -> Transaction {
        let (_, gas_coin) = self.setup_coin();
        TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price)
            .gas_limit(1000)
            .add_input(gas_coin)
            .finalize_as_transaction()
    }

    pub fn setup_coin(&self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng.borrow_mut(), Some(&self.mock_db))
    }
}

mockall::mock! {
    pub P2P {}

    impl PeerToPeer for P2P {
        type GossipedTransaction = GossipedTransaction;

        fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

        fn gossiped_transaction_events(&self) -> BoxStream<GossipedTransaction>;

        fn notify_gossip_transaction_validity(
            &self,
            message_info: GossipsubMessageInfo,
            validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()>;
    }
}

impl MockP2P {
    pub fn new_with_txs(txs: Vec<Transaction>) -> Self {
        let mut p2p = MockP2P::default();
        p2p.expect_gossiped_transaction_events().returning(move || {
            let txs_clone = txs.clone();
            let stream = fuel_core_services::stream::unfold(txs_clone, |mut txs| async {
                let tx = txs.pop();
                if let Some(tx) = tx {
                    Some((GossipData::new(tx, vec![], vec![]), txs))
                } else {
                    core::future::pending().await
                }
            });
            Box::pin(stream)
        });
        p2p.expect_broadcast_transaction()
            .returning(move |_| Ok(()));
        p2p
    }
}

mockall::mock! {
    pub Importer {}

    impl BlockImporter for Importer {
        fn block_events(&self) -> BoxStream<Arc<ImportResult>>;
    }
}

impl MockImporter {
    fn with_blocks(blocks: Vec<SealedBlock>) -> Self {
        let mut importer = MockImporter::default();
        importer.expect_block_events().returning(move || {
            let blocks = blocks.clone();
            let stream = fuel_core_services::stream::unfold(blocks, |mut blocks| async {
                let block = blocks.pop();
                if let Some(sealed_block) = block {
                    let result = ImportResult {
                        sealed_block,
                        tx_status: vec![],
                    };
                    let result = Arc::new(result);
                    Some((result, blocks))
                } else {
                    core::future::pending().await
                }
            });
            Box::pin(stream)
        });
        importer
    }
}

pub struct TestContextBuilder {
    config: Option<Config>,
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
            config: None,
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(10),
            p2p: None,
            importer: None,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
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
            .gas_limit(1000)
            .add_input(gas_coin)
            .finalize_as_transaction()
    }

    pub fn setup_coin(&mut self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng, Some(&self.mock_db))
    }

    pub fn build(self) -> TestContext {
        let rng = RefCell::new(self.rng);
        let config = self.config.unwrap_or_default();
        let mock_db = self.mock_db;

        let p2p = self.p2p.unwrap_or_else(|| MockP2P::new_with_txs(vec![]));
        let importer = self
            .importer
            .unwrap_or_else(|| MockImporter::with_blocks(vec![]));

        let service = new_service(config, mock_db.clone(), importer, p2p);

        TestContext {
            service,
            mock_db,
            rng,
        }
    }

    pub async fn build_and_start(self) -> TestContext {
        let context = self.build();
        context.service.start_and_await().await.unwrap();
        context
    }
}
