#![allow(non_snake_case)]

use super::*;
use fuel_core_services::stream::IntoBoxStream;
use fuel_core_storage::{
    column::Column as OnChainColumn,
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::SealedBlock,
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
