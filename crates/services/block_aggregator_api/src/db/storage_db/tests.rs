#![allow(non_snake_case)]

use super::*;
use crate::db::storage_db::table::Column;
use fuel_core_storage::{
    StorageAsRef,
    StorageMutate,
    structured_storage::test::InMemoryStorage,
    transactional::IntoTransaction,
};
use fuel_core_types::{
    ed25519::signature::rand_core::SeedableRng,
    fuel_merkle::storage::StorageInspectInfallible,
    fuel_types::BlockHeight,
};
use rand::rngs::StdRng;

fn database() -> StorageTransaction<InMemoryStorage<Column>> {
    InMemoryStorage::default().into_transaction()
}

#[tokio::test]
async fn store_block__adds_to_storage() {
    let mut rng = StdRng::seed_from_u64(666);
    // given
    let db = database();
    let mut adapter = StorageDB::new(db.clone());
    let height = BlockHeight::from(1u32);
    let block = Block::random(&mut rng);

    // when
    adapter.store_block(height, block.clone()).await.unwrap();

    // then
    let block = db.storage::<Blocks>().get(&height).unwrap().to_owned();
    assert_eq!(block, Some(block));
}
