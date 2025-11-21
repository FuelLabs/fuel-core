#![allow(non_snake_case)]

use super::*;
use fuel_core_services::stream::{
    IntoBoxStream,
    pending,
};
use fuel_core_storage::{
    StorageAsMut,
    column::Column as OnChainColumn,
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
        WriteTransaction,
    },
};
use futures::StreamExt;
use std::collections::HashMap;

use crate::blocks::importer_and_db_source::{
    serializer_adapter::SerializerAdapter,
    sync_service::TxReceipts,
};
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_tx::{
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
    services::{
        block_importer::ImportResult,
        transaction_status::TransactionStatus,
    },
};
use std::sync::Arc;

fn onchain_db() -> StorageTransaction<InMemoryStorage<OnChainColumn>> {
    InMemoryStorage::default().into_transaction()
}

struct MockTxReceiptsSource {
    receipts_map: HashMap<TxId, Vec<FuelReceipt>>,
}

impl MockTxReceiptsSource {
    fn new(receipts: &[(TxId, Vec<FuelReceipt>)]) -> Self {
        let receipts_map = receipts.iter().cloned().collect();
        Self { receipts_map }
    }
}

impl TxReceipts for MockTxReceiptsSource {
    async fn get_receipts(&self, tx_id: &TxId) -> Result<Vec<FuelReceipt>> {
        let receipts = self.receipts_map.get(tx_id).cloned().ok_or_else(|| {
            Error::BlockSource(anyhow!("no receipts found for a tx with id {}", tx_id))
        })?;
        Ok(receipts)
    }
}

#[tokio::test]
async fn next_block__gets_new_block_from_importer() {
    // given
    let block = SealedBlock::default();
    let height = block.entity.header().height();
    let import_result = Arc::new(
        ImportResult {
            sealed_block: block.clone(),
            tx_status: vec![],
            events: vec![],
            source: Default::default(),
        }
        .wrap(),
    );
    let blocks: Vec<SharedImportResult> = vec![import_result];
    let block_stream = tokio_stream::iter(blocks).chain(pending()).into_boxed();
    let serializer = SerializerAdapter;
    let db = onchain_db();
    let receipt_source = MockTxReceiptsSource::new(&[]);
    let db_starting_height = BlockHeight::from(0u32);
    // we don't need to sync anything, so we can use the same height for both
    let db_ending_height = db_starting_height;
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        db,
        receipt_source,
        db_starting_height,
        db_ending_height,
    );

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block.entity, &[]).unwrap();
    let expected = BlockSourceEvent::NewBlock(*height, serialized);
    assert_eq!(expected, actual);
}

fn arbitrary_block_with_txs(height: BlockHeight) -> FuelBlock {
    let mut block = FuelBlock::default();
    block.header_mut().set_block_height(height);
    let txs = block.transactions_mut();
    *txs = vec![Transaction::default_test_tx()];
    block
}

fn arbitrary_receipts() -> Vec<FuelReceipt> {
    let one = FuelReceipt::Mint {
        sub_id: Default::default(),
        contract_id: Default::default(),
        val: 100,
        pc: 0,
        is: 0,
    };
    let two = FuelReceipt::Transfer {
        id: Default::default(),
        to: Default::default(),
        amount: 50,
        asset_id: Default::default(),
        pc: 0,
        is: 0,
    };
    vec![one, two]
}

#[tokio::test]
async fn next_block__can_get_block_from_db() {
    // given
    let chain_id = ChainId::default();
    let height1 = BlockHeight::from(0u32);
    let height2 = BlockHeight::from(1u32);
    let block = arbitrary_block_with_txs(height1);
    let receipts = arbitrary_receipts();
    let height = block.header().height();
    let serializer = SerializerAdapter;
    let mut onchain_db = onchain_db();
    let mut tx = onchain_db.write_transaction();
    let compressed_block = block.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(height, &compressed_block)
        .unwrap();
    let tx_id = block.transactions()[0].id(&chain_id);
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block.transactions()[0].id(&chain_id),
            &block.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();
    let receipt_source = MockTxReceiptsSource::new(&[(tx_id, receipts.clone())]);
    let block_stream = tokio_stream::pending().into_boxed();
    let db_starting_height = *height;
    let db_ending_height = height2;
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        onchain_db,
        receipt_source,
        db_starting_height,
        db_ending_height,
    );

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block, &receipts).unwrap();
    let expected = BlockSourceEvent::OldBlock(*height, serialized);
    assert_eq!(expected, actual);
}
