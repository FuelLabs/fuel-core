#![allow(non_snake_case)]
use super::{
    storage::GasPrice,
    *,
};
use crate::database::Database;
use fuel_core_gas_price_service::fuel_gas_price_updater::AlgorithmUpdaterV1;
use fuel_core_storage::StorageAsMut;

fn arb_metadata() -> UpdaterMetadata {
    let height = 111231u32.into();
    arb_metadata_with_l2_height(height)
}

fn arb_metadata_with_l2_height(l2_height: BlockHeight) -> UpdaterMetadata {
    AlgorithmUpdaterV1 {
        new_exec_price: 100,
        last_da_gas_price: 34,
        min_exec_gas_price: 12,
        exec_gas_price_change_percent: 2,
        l2_block_height: l2_height.into(),
        l2_block_fullness_threshold_percent: 0,
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        total_da_rewards: 0,
        da_recorded_block_height: 0,
        latest_known_total_da_cost: 0,
        projected_total_da_cost: 0,
        da_p_component: 0,
        da_d_component: 0,
        profit_avg: 0,
        avg_window: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_blocks: vec![],
    }
    .into()
}

fn database() -> Database<GasPrice> {
    Database::default()
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
    let metadata_storage = FuelGasPriceMetadataStorage { database };

    // when
    let actual = metadata_storage.get_metadata(&block_height).await.unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn get_metadata__returns_none_if_does_not_exist() {
    // given
    let database: Database = Database::default();
    let block_height: BlockHeight = 1u32.into();
    let metadata_storage = FuelGasPriceMetadataStorage { database };

    // when
    let actual = metadata_storage.get_metadata(&block_height).await.unwrap();

    // then
    let expected = None;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn set_metadata__can_set_metadata() {
    // given
    let database: Database = Database::default();
    let block_height: BlockHeight = 1u32.into();
    let metadata = arb_metadata_with_l2_height(block_height);
    let mut metadata_storage = FuelGasPriceMetadataStorage { database };

    // when
    let actual = metadata_storage.get_metadata(&block_height).await.unwrap();
    assert_eq!(None, actual);
    metadata_storage
        .set_metadata(metadata.clone())
        .await
        .unwrap();
    let actual = metadata_storage.get_metadata(&block_height).await.unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}
