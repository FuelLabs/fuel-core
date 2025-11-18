#![allow(non_snake_case)]

use super::*;
use crate::{
    blocks::importer_and_db_source::{
        BlockSerializer,
        serializer_adapter::SerializerAdapter,
    },
    db::table::Column,
};
use fuel_core_storage::{
    StorageAsRef,
    structured_storage::test::InMemoryStorage,
    transactional::IntoTransaction,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};
use futures::StreamExt;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

fn proto_block_with_height(height: BlockHeight) -> ProtoBlock {
    let serializer_adapter = SerializerAdapter;
    let mut default_block = FuelBlock::<Transaction>::default();
    default_block.header_mut().set_block_height(height);
    serializer_adapter.serialize_block(&default_block).unwrap()
}

#[tokio::test]
async fn store_block__adds_to_storage() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));
    let height = BlockHeight::from(1u32);
    let expected = proto_block_with_height(height);
    let block = BlockSourceEvent::OldBlock(height, expected.clone());

    // when
    adapter.store_block(block).await.unwrap();

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

    let expected_1 = proto_block_with_height(height_1);
    let expected_2 = proto_block_with_height(height_2);
    let expected_3 = proto_block_with_height(height_3);

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
    let adapter = StorageDB::new(tx, BlockHeight::from(0u32));

    // when
    let BlockRangeResponse::Literal(stream) =
        adapter.get_block_range(height_2, height_3).await.unwrap()
    else {
        panic!("expected literal response")
    };
    let actual = stream.collect::<Vec<_>>().await;

    // then
    assert_eq!(actual, vec![(height_2, expected_2), (height_3, expected_3)]);
}

#[tokio::test]
async fn store_block__updates_the_highest_continuous_block_if_contiguous() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));
    let height = BlockHeight::from(1u32);
    let expected = proto_block_with_height(height);
    let block = BlockSourceEvent::OldBlock(height, expected.clone());

    // when
    adapter.store_block(block).await.unwrap();

    // then
    let expected = height;
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__does_not_update_the_highest_continuous_block_if_not_contiguous() {
    // given
    let mut db = database();
    let mut tx = db.write_transaction();
    let starting_height = BlockHeight::from(1u32);
    tx.storage_as_mut::<LatestBlock>()
        .insert(&(), &starting_height)
        .unwrap();
    tx.commit().unwrap();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));
    let height = BlockHeight::from(3u32);
    let proto = proto_block_with_height(height);
    let block = BlockSourceEvent::NewBlock(height, proto.clone());

    // when
    adapter.store_block(block).await.unwrap();

    // then
    let expected = starting_height;
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn store_block__updates_the_highest_continuous_block_if_filling_a_gap() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));

    for height in 2..=10u32 {
        let height = BlockHeight::from(height);
        let block = proto_block_with_height(height);
        let block = BlockSourceEvent::NewBlock(height, block.clone());
        adapter.store_block(block).await.unwrap();
    }
    // when
    let height = BlockHeight::from(1u32);
    let some_block = proto_block_with_height(height);
    let block = BlockSourceEvent::OldBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(10u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);
}
#[tokio::test]
async fn store_block__new_block_updates_the_highest_continuous_block_if_synced() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));

    let height = BlockHeight::from(0u32);
    let some_block = proto_block_with_height(height);
    let block = BlockSourceEvent::OldBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // when
    let height = BlockHeight::from(1u32);
    let some_block = proto_block_with_height(height);
    let block = BlockSourceEvent::NewBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(1u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);

    assert!(adapter.synced)
}

#[tokio::test]
async fn store_block__new_block_comes_first() {
    // given
    let db = database();
    let mut adapter = StorageDB::new(db, BlockHeight::from(0u32));

    // when
    let height = BlockHeight::from(0u32);
    let some_block = proto_block_with_height(height);
    let block = BlockSourceEvent::NewBlock(height, some_block.clone());
    adapter.store_block(block).await.unwrap();

    // then
    let expected = BlockHeight::from(0u32);
    let actual = adapter.get_current_height().await.unwrap().unwrap();
    assert_eq!(expected, actual);

    assert!(adapter.synced);
}
