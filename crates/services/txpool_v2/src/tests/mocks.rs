use crate::{
    error::Error,
    ports::{
        AtomicView,
        BlockImporter as BlockImporterTrait,
        ConsensusParametersProvider,
        GasPriceProvider,
        MemoryPool as MemoryPoolTrait,
        TxPoolPersistentStorage,
        WasmChecker,
        WasmValidityError,
        P2P as P2PTrait,
    },
    GasPrice,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        SealedBlock,
    },
    entities::{
        coins::coin::{
            Coin,
            CompressedCoin,
        },
        relayer::message::Message,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ConsensusParameters,
        Contract,
        ContractId,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::{
        interpreter::MemoryInstance,
        BlobBytes,
    },
    services::{
        block_importer::{
            ImportResult,
            SharedImportResult,
        },
        p2p::{
            GossipData,
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            PeerId,
        },
    },
};
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub coins: HashMap<UtxoId, CompressedCoin>,
    pub contracts: HashMap<ContractId, Contract>,
    pub blobs: HashMap<BlobId, BlobBytes>,
    pub messages: HashMap<Nonce, Message>,
    pub spent_messages: HashSet<Nonce>,
}

#[derive(Clone, Default)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
    pub fn insert_coin(&self, coin: Coin) {
        self.data
            .lock()
            .unwrap()
            .coins
            .insert(coin.utxo_id, coin.compress());
    }

    pub fn insert_dummy_blob(&self, blob_id: BlobId) {
        self.data
            .lock()
            .unwrap()
            .blobs
            .insert(blob_id, vec![123; 123].into());
    }

    pub fn insert_message(&self, message: Message) {
        self.data
            .lock()
            .unwrap()
            .messages
            .insert(*message.id(), message);
    }

    pub fn spend_message(&self, id: Nonce) {
        self.data.lock().unwrap().spent_messages.insert(id);
    }
}

impl TxPoolPersistentStorage for MockDb {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>> {
        Ok(self.data.lock().unwrap().coins.get(utxo_id).cloned())
    }

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .contracts
            .contains_key(contract_id))
    }

    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool> {
        Ok(self.data.lock().unwrap().blobs.contains_key(blob_id))
    }

    fn message(&self, id: &Nonce) -> StorageResult<Option<Message>> {
        Ok(self.data.lock().unwrap().messages.get(id).cloned())
    }
}

pub struct MockDBProvider(pub MockDb);

impl AtomicView for MockDBProvider {
    type LatestView = MockDb;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.0.clone())
    }
}

#[derive(Debug, Clone)]
pub struct MockTxPoolGasPrice {
    pub gas_price: Option<GasPrice>,
}

impl MockTxPoolGasPrice {
    pub fn new(gas_price: GasPrice) -> Self {
        Self {
            gas_price: Some(gas_price),
        }
    }

    pub fn new_none() -> Self {
        Self { gas_price: None }
    }
}

#[async_trait::async_trait]
impl GasPriceProvider for MockTxPoolGasPrice {
    async fn next_gas_price(&self) -> Result<GasPrice, Error> {
        self.gas_price
            .ok_or(Error::GasPriceNotFound("Gas price not found".to_string()))
    }
}

pub struct MockWasmChecker {
    pub result: Result<(), WasmValidityError>,
}

impl MockWasmChecker {
    pub fn new(result: Result<(), WasmValidityError>) -> Self {
        Self { result }
    }
}

impl WasmChecker for MockWasmChecker {
    fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        self.result
    }
}

pub struct MockMemoryPool;

#[async_trait::async_trait]
impl MemoryPoolTrait for MockMemoryPool {
    type Memory = MemoryInstance;

    async fn get_memory(&self) -> Self::Memory {
        MemoryInstance::new()
    }
}

mockall::mock! {
    pub ConsensusParametersProvider {}

    impl ConsensusParametersProvider for ConsensusParametersProvider {
        fn latest_consensus_parameters(&self) -> (ConsensusParametersVersion, Arc<ConsensusParameters>);
    }
}

type GossipedTransaction = GossipData<Transaction>;

mockall::mock! {
    pub P2P {}

    #[async_trait::async_trait]
    impl P2PTrait for P2P {
        type GossipedTransaction = GossipedTransaction;

        fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

        fn gossiped_transaction_events(&self) -> BoxStream<GossipedTransaction>;

        fn notify_gossip_transaction_validity(
            &self,
            message_info: GossipsubMessageInfo,
            validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()>;

        fn subscribe_new_peers(&self) -> BoxStream<PeerId>;

        async fn request_tx_ids(&self, peer_id: PeerId) -> anyhow::Result<Vec<TxId>>;

        async fn request_txs(
            &self,
            peer_id: PeerId,
            tx_ids: Vec<TxId>,
        ) -> anyhow::Result<Vec<Option<Transaction>>>;
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

        p2p
    }
}

mockall::mock! {
    pub Importer {}

    impl BlockImporterTrait for Importer {
        fn block_events(&self) -> BoxStream<SharedImportResult>;
    }
}

impl MockImporter {
    pub fn with_blocks(blocks: Vec<SealedBlock>) -> Self {
        let mut importer = MockImporter::default();
        importer.expect_block_events().returning(move || {
            let blocks = blocks.clone();
            let stream = fuel_core_services::stream::unfold(blocks, |mut blocks| async {
                let block = blocks.pop();
                if let Some(sealed_block) = block {
                    let result: SharedImportResult = Arc::new(
                        ImportResult::new_from_local(sealed_block, vec![], vec![]),
                    );

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
