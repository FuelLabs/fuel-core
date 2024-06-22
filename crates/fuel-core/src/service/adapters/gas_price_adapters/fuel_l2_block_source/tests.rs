#![allow(non_snake_case)]

use super::*;
use fuel_core::database::Database;
use fuel_core_storage::StorageAsMut;
use fuel_core_types::blockchain::block::CompressedBlock;
use futures::future::{
    maybe_done,
    MaybeDone,
};

fn l2_source(database: Database) -> FuelL2BlockSource<Database> {
    FuelL2BlockSource {
        frequency: Duration::from_millis(10),
        database,
    }
}

fn l2_source_with_frequency(
    database: Database,
    frequency: Duration,
) -> FuelL2BlockSource<Database> {
    FuelL2BlockSource {
        frequency,
        database,
    }
}

#[tokio::test]
async fn get_l2_block__gets_expected_value() {
    // given
    let block = CompressedBlock::default();
    let block_height = 1u32.into();
    let block_info = get_block_info(&block);
    let mut database = Database::default();
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();
    let source = l2_source(database);

    // when
    let result = source.get_l2_block(block_height).await.unwrap();

    // then
    assert_eq!(result, block_info);
}

#[tokio::test]
async fn get_l2_block__waits_for_block() {
    // given
    let block_height = 1u32.into();
    let block = CompressedBlock::default();
    let mut database = Database::default();
    let frequency = Duration::from_millis(10);
    let source = l2_source_with_frequency(database.clone(), frequency);

    // when
    let mut fut_l2_block = source.get_l2_block(block_height);
    for _ in 0..10 {
        fut_l2_block = match maybe_done(fut_l2_block) {
            MaybeDone::Future(fut) => {
                tokio::time::sleep(frequency).await;
                fut
            }
            _ => panic!("Shouldn't be done yet"),
        };
    }
    database
        .storage_as_mut::<FuelBlocks>()
        .insert(&block_height, &block)
        .unwrap();

    // then
    let actual = fut_l2_block.await.unwrap();
    let expected = get_block_info(&block);
    assert_eq!(expected, actual);
}
