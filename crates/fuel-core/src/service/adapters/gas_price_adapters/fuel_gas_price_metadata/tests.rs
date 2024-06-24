#![allow(non_snake_case)]
use super::*;
use fuel_core::database::Database;
use fuel_core_gas_price_service::fuel_gas_price_updater::AlgorithmUpdaterV1;
use fuel_core_storage::StorageAsMut;

#[tokio::test]
async fn get_metadata__can_get_most_recent_version() {
    // given
    let mut database: Database = Database::default();
    let block_height: BlockHeight = 1u32.into();
    let metadata = AlgorithmUpdaterV1 {
        new_exec_price: 0,
        last_da_gas_price: 0,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 0,
        l2_block_height: 0,
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
    .into();
    database
        .storage_as_mut::<GasPriceMetadata>()
        .insert(&block_height, &metadata)
        .unwrap();
    let metadata_storage = FuelGasPriceMetadataStorage {
        _database: database,
    };

    // when
    let actual = metadata_storage.get_metadata(&block_height).await.unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}
