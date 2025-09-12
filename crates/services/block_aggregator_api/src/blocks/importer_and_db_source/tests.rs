#![allow(non_snake_case)]

use super::*;
use fuel_core_services::stream::IntoBoxStream;
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

use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
    services::block_importer::ImportResult,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct MockSerializer;

impl BlockSerializer for MockSerializer {
    fn serialize_block(&self, block: &FuelBlock) -> Result<Block> {
        let bytes_vec = postcard::to_allocvec(block).map_err(|e| {
            Error::BlockSource(anyhow!("failed to serialize block: {}", e))
        })?;
        Ok(Block::from(bytes_vec))
    }
}

fn database() -> StorageTransaction<InMemoryStorage<OnChainColumn>> {
    InMemoryStorage::default().into_transaction()
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
    let block_stream = tokio_stream::iter(blocks).into_boxed();
    let serializer = MockSerializer;
    let db = database();
    let db_starting_height = BlockHeight::from(0u32);
    let db_ending_height = BlockHeight::from(1u32);
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        db,
        db_starting_height,
        db_ending_height,
    );

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block.entity).unwrap();
    let expected = BlockSourceEvent::NewBlock(*height, serialized);
    assert_eq!(expected, actual);
}

fn arbitrary_block_with_txs() -> FuelBlock {
    let mut block = FuelBlock::default();
    let txs = block.transactions_mut();
    *txs = vec![Transaction::default_test_tx()];
    block
}

#[tokio::test]
async fn next_block__can_get_block_from_db() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    // given
    let chain_id = ChainId::default();
    let block = arbitrary_block_with_txs();
    let height = block.header().height();
    let serializer = MockSerializer;
    let mut db = database();
    let mut tx = db.write_transaction();
    let compressed_block = block.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(&height, &compressed_block)
        .unwrap();
    tx.commit().unwrap();
    let mut tx = db.write_transaction();
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block.transactions()[0].id(&chain_id),
            &block.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();
    let block_stream = tokio_stream::pending().into_boxed();
    let db_starting_height = *height;
    let db_ending_height = *height;
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        db,
        db_starting_height,
        db_ending_height,
    );

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block).unwrap();
    let expected = BlockSourceEvent::OldBlock(*height, serialized);
    assert_eq!(expected, actual);
}
