#![allow(non_snake_case)]

use super::*;
use crate::{
    blocks::old_block_source::{
        BlockConverter,
        convertor_adapter::ProtobufBlockConverter,
    },
    db::table::{
        Column,
        Mode,
    },
};
use fuel_core_storage::{
    StorageAsRef,
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use std::sync::Arc;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn proto_block_bytes_with_height(height: BlockHeight) -> Arc<Vec<u8>> {
    let convertor_adapter = ProtobufBlockConverter;
    let mut default_block = FuelBlock::<Transaction>::default();
    default_block.header_mut().set_block_height(height);
    convertor_adapter
        .convert_block(&default_block, &[])
        .unwrap()
}

#[tokio::test]
async fn store_block__adds_to_storage() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db);
    let height = BlockHeight::from(1u32);
    let expected = proto_block_bytes_with_height(height);

    // when
    adapter.store_block(height, &expected).await.unwrap();

    // then
    let actual = adapter
        .storage
        .storage_as_ref::<Blocks>()
        .get(&height)
        .unwrap()
        .unwrap()
        .into_owned();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn get_block__can_get_expected_range() {
    // given
    let mut db = database();
    let height_1 = BlockHeight::from(1u32);
    let height_2 = BlockHeight::from(2u32);
    let height_3 = BlockHeight::from(3u32);

    let expected_1 = proto_block_bytes_with_height(height_1);
    let expected_2 = proto_block_bytes_with_height(height_2);
    let expected_3 = proto_block_bytes_with_height(height_3);

    let mut tx = db.write_transaction();
    tx.storage_as_mut::<Blocks>()
        .insert(&height_1, &expected_1)
        .unwrap();
    tx.storage_as_mut::<Blocks>()
        .insert(&height_2, &expected_2)
        .unwrap();
    tx.storage_as_mut::<Blocks>()
        .insert(&height_3, &expected_3)
        .unwrap();
    tx.commit().unwrap();
    let db = db.commit().unwrap();
    let tx = db.into_transaction();
    let adapter = StorageBlocksProvider::new(tx);

    // when
    let BlockRangeResponse::Bytes(stream) =
        adapter.get_block_range(height_2, height_3).unwrap()
    else {
        panic!("expected bytes response")
    };
    let actual = stream.collect::<Vec<_>>().await;

    // then
    assert_eq!(actual, vec![(height_2, expected_2), (height_3, expected_3)]);
}

#[tokio::test]
async fn store_block__updates_continuous_block_if_contiguous() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db);
    let height = BlockHeight::from(1u32);
    let expected = proto_block_bytes_with_height(height);

    // when
    adapter.store_block(height, &expected).await.unwrap();

    // then
    let expected = height;
    let actual = adapter.get_current_height().unwrap().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__fails_if_not_contiguous() {
    // Given
    let mut db = database();
    let mut tx = db.write_transaction();
    let starting_height = BlockHeight::from(1u32);
    tx.storage_as_mut::<LatestBlock>()
        .insert(&(), &Mode::Local(starting_height))
        .unwrap();
    tx.commit().unwrap();
    let mut adapter = StorageDB::new(db);
    let height = BlockHeight::from(3u32);
    let proto = proto_block_bytes_with_height(height);

    // when
    let result = adapter.store_block(height, &proto).await;

    // then
    result.expect_err("expected error");
}
