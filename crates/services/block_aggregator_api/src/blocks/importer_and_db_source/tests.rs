use super::*;
use fuel_core_services::stream::IntoBoxStream;
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_types::BlockHeight,
    services::block_importer::ImportResult,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct MockSerializer;

impl BlockSerializer for MockSerializer {
    fn serialize_block(&self, _block: &FuelBlock) -> Result<Block> {
        todo!()
    }
}

#[tokio::test]
async fn next_block__gets_new_block_from_importer() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    // given
    let height = BlockHeight::from(123u32);
    let block = SealedBlock::default();
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
    let mut adapter = ImporterAndDbSource::new(block_stream, serializer.clone());

    // when
    let actual = adapter.next_block().await.unwrap();

    // then
    let serialized = serializer.serialize_block(&block).unwrap();
    let expected = BlockSourceEvent::NewBlock(height, serialized);
    assert_eq!(expected, actual);
}
