#![allow(non_snake_case)]

use super::*;
use fuel_core::database::Database;
use fuel_core_storage::StorageAsMut;
use fuel_core_types::blockchain::block::CompressedBlock;

fn l2_source(database: Database) -> FuelL2BlockSource<Database> {
    FuelL2BlockSource {
        frequency: Duration::from_millis(10),
        on_chain: database,
    }
}

#[tokio::test]
async fn get_l2_block__gets_expected_value() {
    let block = CompressedBlock::default();
    let block_height = 1u32.into();
    let block_info = get_block_info(&block);
    let mut database = Database::default();
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();

    let source = l2_source(database);

    let result = source.get_l2_block(block_height).await.unwrap();
    assert_eq!(result, block_info);
}
