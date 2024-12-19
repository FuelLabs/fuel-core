use crate::{
    ports::{
        AtomicView,
        BlockImporter as BlockImporterTrait,
        ConsensusParametersProvider,
        GasPriceProvider,
        NotifyP2P,
        P2PRequests,
        P2PSubscriptions,
        TxPoolPersistentStorage,
        WasmChecker,
        WasmValidityError,
    },
    GasPrice,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    Mappable,
    PredicateStorageRequirements,
    Result as StorageResult,
    StorageInspect,
    StorageRead,
    StorageSize,
};
use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        SealedBlock,
    },
    entities::{
        coins::coin::CompressedCoin,
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
        BlobBytes,
        BlobData,
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
    borrow::Cow,
    collections::HashMap,
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
}

#[derive(Clone, Default)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
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

impl StorageRead<BlobData> for MockDb {
    fn read(
        &self,
        key: &<BlobData as Mappable>::Key,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<Option<usize>, Self::Error> {
        let table = self.data.lock().unwrap();
        let bytes = table.blobs.get(key);

        let len = bytes.map(|bytes| {
            let len = bytes
                .0
                .len()
                .checked_sub(offset)
                .expect("Read offset larger than value size");
            buf.copy_from_slice(&bytes.0.as_slice()[offset..]);
            len
        });
        Ok(len)
    }

    fn read_alloc(
        &self,
        key: &<BlobData as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let table = self.data.lock().unwrap();
        let bytes = table.blobs.get(key);
        let bytes = bytes.map(|bytes| bytes.clone().0);
        Ok(bytes)
    }
}

impl StorageInspect<BlobData> for MockDb {
    type Error = ();

    fn get(
        &self,
        key: &<BlobData as Mappable>::Key,
    ) -> Result<Option<Cow<<BlobData as Mappable>::OwnedValue>>, Self::Error> {
        let table = self.data.lock().unwrap();
        let bytes = table.blobs.get(key);
        Ok(bytes.map(|b| Cow::Owned(b.clone())))
    }

    fn contains_key(
        &self,
        key: &<BlobData as Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().blobs.contains_key(key))
    }
}

impl StorageSize<BlobData> for MockDb {
    fn size_of_value(
        &self,
        key: &<BlobData as Mappable>::Key,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .blobs
            .get(key)
            .map(|blob| blob.0.len()))
    }
}

impl PredicateStorageRequirements for MockDb {
    fn storage_error_to_string(error: Self::Error) -> String {
        format!("{:?}", error)
    }
}

#[derive(Clone)]
pub struct MockDBProvider(pub MockDb);

impl AtomicView for MockDBProvider {
    type LatestView = MockDb;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.0.clone())
    }
}

#[derive(Debug, Clone)]
pub struct MockTxPoolGasPrice {
    pub gas_price: GasPrice,
}

impl MockTxPoolGasPrice {
    pub fn new(gas_price: GasPrice) -> Self {
        Self { gas_price }
    }
}

impl GasPriceProvider for MockTxPoolGasPrice {
    fn next_gas_price(&self) -> GasPrice {
        self.gas_price
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

mockall::mock! {
    pub ConsensusParametersProvider {}

    impl ConsensusParametersProvider for ConsensusParametersProvider {
        fn latest_consensus_parameters(&self) -> (ConsensusParametersVersion, Arc<ConsensusParameters>);
    }
}

type GossipedTransaction = GossipData<Transaction>;

mockall::mock! {
    pub P2P {}

    impl P2PSubscriptions for P2P {
        type GossipedTransaction = GossipedTransaction;

        fn gossiped_transaction_events(&self) -> BoxStream<GossipedTransaction>;

        fn subscribe_new_peers(&self) -> BoxStream<PeerId>;
    }

    impl NotifyP2P for P2P {
        fn notify_gossip_transaction_validity(
            &self,
            message_info: GossipsubMessageInfo,
            validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()>;

        fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;
    }

    #[async_trait::async_trait]
    impl P2PRequests for P2P {
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
