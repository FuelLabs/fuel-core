use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_storage::{
    column::Column,
    kv_store::StorageColumn,
};
use fuel_core_types::fuel_tx::Bytes32;
use rand::SeedableRng;
use test_helpers::counter_contract;

/// Create a counter contract.
/// Increment it multiple times, and make sure the replay gives correct storage state every time.
#[tokio::test(flavor = "multi_thread")]
async fn storage_read_replay__returns_counter_state() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);

    // given
    let mut node_config = Config::local_node();
    node_config.debug = true;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let (block_height, contract_id) = counter_contract::deploy(&client, &mut rng).await;

    let _replay = client
        .storage_read_replay(&block_height)
        .await
        .expect("Failed to replay storage read");

    let mut storage_slot_key = contract_id.to_vec();
    storage_slot_key.extend(Bytes32::zeroed().to_vec());

    for i in 0..10u64 {
        // when
        let (block_height, value_after) =
            counter_contract::increment(&client, &mut rng, contract_id).await;

        let replay = client
            .storage_read_replay(&block_height)
            .await
            .expect("Failed to replay storage read");

        // then
        assert_eq!(value_after, i + 1, "Counter value mismatch");

        let storage_bytes = replay
            .iter()
            .find(|item| {
                item.column == Column::ContractsState.id() && item.key == storage_slot_key
            })
            .expect("No storage read found")
            .value
            .clone()
            .expect("Storage read was unexpectedly empty");

        assert_eq!(
            i,
            counter_contract::value_from_storage_bytes(&storage_bytes),
            "Counter value mismatch"
        );
    }
}
