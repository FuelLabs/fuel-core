#![allow(non_snake_case)]

use super::*;
use crate::db::storage_db::table::Column;
use fuel_core_storage::{
    StorageAsRef,
    structured_storage::test::InMemoryStorage,
    transactional::IntoTransaction,
};
use fuel_core_types::{
    ed25519::signature::rand_core::SeedableRng,
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use rand::rngs::StdRng;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

#[tokio::test]
async fn store_block__adds_to_storage() {
    let mut rng = StdRng::seed_from_u64(666);
    // given
    let db = database();
    let mut adapter = StorageDB::new(db);
    let height = BlockHeight::from(1u32);
    let expected = Block::random(&mut rng);

    // when
    adapter.store_block(height, expected.clone()).await.unwrap();

    // then
    let actual = adapter
        .inner
        .storage_as_ref::<Blocks>()
        .get(&height)
        .unwrap()
        .unwrap()
        .into_owned();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn get_block__can_get_expected_range() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    let mut rng = StdRng::seed_from_u64(666);
    // given
    let mut db = database();
    let height_1 = BlockHeight::from(1u32);
    let height_2 = BlockHeight::from(2u32);
    let height_3 = BlockHeight::from(3u32);
    let expected_1 = Block::random(&mut rng);
    let expected_2 = Block::random(&mut rng);
    let expected_3 = Block::random(&mut rng);

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
    let mut adapter = StorageDB::new(tx);

    // when
    let BlockRangeResponse::Literal(stream) =
        adapter.get_block_range(height_2, height_3).await.unwrap()
    else {
        panic!("expected literal response")
    };
    let actual = stream.collect::<Vec<_>>().await;

    // then
    assert_eq!(actual, vec![expected_2, expected_3]);
}
