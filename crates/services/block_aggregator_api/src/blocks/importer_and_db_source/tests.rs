#![allow(non_snake_case)]

use super::*;
use ::postcard::to_allocvec;
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
use std::collections::HashSet;

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
        let bytes_vec = to_allocvec(block).map_err(|e| {
            Error::BlockSource(anyhow!("failed to serialize block: {}", e))
        })?;
        Ok(Block::from(bytes_vec))
    }
}

fn database() -> StorageTransaction<InMemoryStorage<OnChainColumn>> {
    InMemoryStorage::default().into_transaction()
}

// let block_stream = tokio_stream::iter(blocks).chain(pending()).into_boxed();
fn stream_with_pending<T: Send + Sync + 'static>(items: Vec<T>) -> BoxStream<T> {
    tokio_stream::iter(items).chain(pending()).into_boxed()
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
    let serializer = MockSerializer;
    let db = database();
    let db_starting_height = BlockHeight::from(0u32);
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        db,
        db_starting_height,
        None,
    );

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block.entity).unwrap();
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

#[tokio::test]
async fn next_block__can_get_block_from_db() {
    // given
    let chain_id = ChainId::default();
    let height1 = BlockHeight::from(0u32);
    let height2 = BlockHeight::from(1u32);
    let block = arbitrary_block_with_txs(height1);
    let height = block.header().height();
    let serializer = MockSerializer;
    let mut db = database();
    let mut tx = db.write_transaction();
    let compressed_block = block.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(height, &compressed_block)
        .unwrap();
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block.transactions()[0].id(&chain_id),
            &block.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();
    let block_stream = tokio_stream::pending().into_boxed();
    let db_starting_height = *height;
    let db_ending_height = Some(height2);
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

#[tokio::test]
async fn next_block__will_sync_blocks_from_db_after_receiving_height_from_new_end() {
    // given
    let chain_id = ChainId::default();
    let height1 = BlockHeight::from(0u32);
    let height2 = BlockHeight::from(1u32);
    let height3 = BlockHeight::from(2u32);
    let block1 = arbitrary_block_with_txs(height1);
    let block2 = arbitrary_block_with_txs(height2);
    let serializer = MockSerializer;
    let mut db = database();
    let mut tx = db.write_transaction();
    let compressed_block = block1.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(&height1, &compressed_block)
        .unwrap();
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block1.transactions()[0].id(&chain_id),
            &block1.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();
    let mut tx = db.write_transaction();
    let compressed_block = block2.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(&height2, &compressed_block)
        .unwrap();
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block2.transactions()[0].id(&chain_id),
            &block2.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();

    // Add the imported block to db as well as streaming
    let block3 = arbitrary_block_with_txs(height3);
    let mut tx = db.write_transaction();
    let compressed_block = block3.compress(&chain_id);
    tx.storage_as_mut::<FuelBlocks>()
        .insert(&height3, &compressed_block)
        .unwrap();
    tx.storage_as_mut::<Transactions>()
        .insert(
            &block3.transactions()[0].id(&chain_id),
            &block3.transactions()[0],
        )
        .unwrap();
    tx.commit().unwrap();

    let sealed_block = SealedBlock {
        entity: block3.clone(),
        consensus: Default::default(),
    };
    let import_result = Arc::new(
        ImportResult {
            sealed_block,
            tx_status: vec![],
            events: vec![],
            source: Default::default(),
        }
        .wrap(),
    );
    let blocks: Vec<SharedImportResult> = vec![import_result];
    let block_stream = stream_with_pending(blocks);
    let db_starting_height = height1;
    let mut adapter = ImporterAndDbSource::new(
        block_stream,
        serializer.clone(),
        db,
        db_starting_height,
        None,
    );

    // when
    let actual1 = adapter.next_block().await.unwrap();
    let actual2 = adapter.next_block().await.unwrap();
    let actual3 = adapter.next_block().await.unwrap();

    // then
    let actual = vec![actual1, actual2, actual3]
        .into_iter()
        .collect::<HashSet<_>>();
    // should receive the
    let expected = vec![
        BlockSourceEvent::OldBlock(height1, serializer.serialize_block(&block1).unwrap()),
        BlockSourceEvent::OldBlock(height2, serializer.serialize_block(&block2).unwrap()),
        BlockSourceEvent::NewBlock(height3, serializer.serialize_block(&block3).unwrap()),
    ];
    let expected: HashSet<_> = expected.into_iter().collect();
    let length = actual.len();
    let expected_length = expected.len();
    for event in &actual {
        tracing::debug!("actual event: {:?}", event);
    }
    assert_eq!(length, expected_length);
    assert_eq!(expected, actual);
}
