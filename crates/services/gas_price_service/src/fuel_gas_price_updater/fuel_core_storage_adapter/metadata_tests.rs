#![allow(non_snake_case)]

use crate::fuel_gas_price_updater::{
    fuel_core_storage_adapter::storage::GasPriceColumn,
    AlgorithmUpdater,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
    StorageAsMut,
};
use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

use super::*;

fn arb_metadata() -> UpdaterMetadata {
    let height = 111231u32.into();
    arb_metadata_with_l2_height(height)
}

fn arb_metadata_with_l2_height(l2_height: BlockHeight) -> UpdaterMetadata {
    let inner = AlgorithmUpdaterV0 {
        new_exec_price: 100,
        min_exec_gas_price: 12,
        exec_gas_price_change_percent: 2,
        l2_block_height: l2_height.into(),
        l2_block_fullness_threshold_percent: 0,
    };
    AlgorithmUpdater::V0(inner).into()
}

fn database() -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
    InMemoryStorage::default().into_transaction()
}

#[tokio::test]
async fn get_metadata__can_get_most_recent_version() {
    // given
    let mut database = database();
    let block_height: BlockHeight = 1u32.into();
    let metadata = arb_metadata();
    database
        .storage_as_mut::<GasPriceMetadata>()
        .insert(&block_height, &metadata)
        .unwrap();

    // when
    let actual = database.get_metadata(&block_height).unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn get_metadata__returns_none_if_does_not_exist() {
    // given
    let database = database();
    let block_height: BlockHeight = 1u32.into();

    // when
    let actual = database.get_metadata(&block_height).unwrap();

    // then
    let expected = None;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn set_metadata__can_set_metadata() {
    // given
    let mut database = database();
    let block_height: BlockHeight = 1u32.into();
    let metadata = arb_metadata_with_l2_height(block_height);

    // when
    let actual = database.get_metadata(&block_height).unwrap();
    assert_eq!(None, actual);
    database.set_metadata(metadata.clone()).unwrap();
    let actual = database.get_metadata(&block_height).unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}
