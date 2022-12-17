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
};
use std::{
    any::Any,
    cell::RefCell,
};
use tokio::sync::mpsc::Receiver;

pub struct TestContext {
    pub(crate) service: Service,
    mock_db: Box<MockDb>,
    _drop_resources: Vec<Box<dyn Any>>,
    rng: RefCell<StdRng>,
    pub(crate) p2p_request_rx: Mutex<Receiver<P2pRequestEvent>>,
    pub(crate) gossip_tx: broadcast::Sender<TransactionGossipData>,
}

impl TestContext {
    pub async fn new() -> Self {
        let rng = RefCell::new(StdRng::seed_from_u64(0));
        let config = Config::default();
        let mock_db = Box::<MockDb>::default();
        let (block_tx, block_rx) = broadcast::channel(10);
        let (p2p_request_tx, p2p_request_rx) = mpsc::channel(100);
        let (gossip_tx, gossip_rx) = broadcast::channel(100);
        let status_tx = TxStatusChange::new(100);
        let (txpool_tx, txpool_rx) = Sender::channel(100);

        let mut builder = ServiceBuilder::new();
        builder
            .config(config)
            .db(mock_db.clone())
            .incoming_tx_receiver(gossip_rx)
            .import_block_event(block_rx)
            .tx_status_sender(status_tx)
            .txpool_sender(txpool_tx)
            .txpool_receiver(txpool_rx)
            .network_sender(p2p_request_tx);

        let service = builder.build().unwrap();
        service.start().await.unwrap();

        // resources to keep alive for the during of the test context
        let drop_resources: Vec<Box<dyn Any>> = vec![Box::new(block_tx)];

        Self {
            service,
            mock_db,
            _drop_resources: drop_resources,
            rng,
            p2p_request_rx: Mutex::new(p2p_request_rx),
            gossip_tx,
        }
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
