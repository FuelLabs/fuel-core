use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_storage::{
    column::Column,
    kv_store::StorageColumn,
};
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_vm::ProgramState,
    services::executor::TransactionExecutionResult,
};
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

/// Similar test as above, but using the dry run instead of a real committed block.
#[tokio::test(flavor = "multi_thread")]
async fn dry_run__storage_read_replay__returns_counter_state() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);

    // given
    let mut node_config = Config::local_node();
    node_config.debug = true;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let (block_height_deployed, contract_id) =
        counter_contract::deploy(&client, &mut rng).await;

    let mut storage_slot_key = contract_id.to_vec();
    storage_slot_key.extend(Bytes32::zeroed().to_vec());

    // create some checkpoints so we can test historical dry run as well
    let (block_height_incr_1, value_incr_1) =
        counter_contract::increment(&client, &mut rng, contract_id).await;
    let (block_height_incr_2, value_incr_2) =
        counter_contract::increment(&client, &mut rng, contract_id).await;
    assert_eq!(value_incr_1, 1, "Counter value mismatch");
    assert_eq!(value_incr_2, 2, "Counter value mismatch");

    for (height, expected_value) in [
        (None, 2), // latest
        (Some(block_height_deployed), 0),
        (Some(block_height_incr_1), 1),
        (Some(block_height_incr_2), 2),
    ] {
        // when
        let tx = counter_contract::increment_tx(&mut rng, contract_id);
        let (statuses, storage_reads) = client
            .dry_run_opt_record_storage_reads(
                &[tx],
                None,
                None,
                height.map(|h| h.succ().unwrap()),
            )
            .await
            .unwrap();

        // then
        assert_eq!(statuses.len(), 1, "Expected one transaction in dry run",);
        let TransactionExecutionResult::Success { result, .. } = statuses[0].result
        else {
            panic!("Expected transaction to be successful");
        };
        let ProgramState::Return(value) = result.expect("Result was unexpectedly empty")
        else {
            panic!("Expected return value");
        };

        // Dry run should increment the counter by one
        assert_eq!(
            value,
            expected_value + 1,
            "Counter value mismatch at height {height:?}"
        );

        let storage_bytes = storage_reads
            .iter()
            .find(|item| {
                item.column == Column::ContractsState.id() && item.key == storage_slot_key
            })
            .expect("No storage read found")
            .value
            .clone()
            .expect("Storage read was unexpectedly empty");

        // The read event should contain the original value
        assert_eq!(
            expected_value,
            counter_contract::value_from_storage_bytes(&storage_bytes),
            "Counter value from storage events mismatch"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run__storage_read_replay__multiple_dry_runs_keep_original_reads() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);

    // given
    let mut node_config = Config::local_node();
    node_config.debug = true;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let (_, contract_id) = counter_contract::deploy(&client, &mut rng).await;

    let mut storage_slot_key = contract_id.to_vec();
    storage_slot_key.extend(Bytes32::zeroed().to_vec());

    // when
    let tx1 = counter_contract::increment_tx(&mut rng, contract_id);
    let tx2 = counter_contract::increment_tx(&mut rng, contract_id);
    let tx3 = counter_contract::increment_tx(&mut rng, contract_id);
    let (statuses, storage_reads) = client
        .dry_run_opt_record_storage_reads(&[tx1, tx2, tx3], None, None, None)
        .await
        .unwrap();

    // then
    assert_eq!(statuses.len(), 3, "Expected three transactions in dry run");
    for i in 0..3 {
        let TransactionExecutionResult::Success { result, .. } =
            statuses[i as usize].result
        else {
            panic!("Expected transaction to be successful");
        };
        let ProgramState::Return(value) = result.expect("Result was unexpectedly empty")
        else {
            panic!("Expected return value");
        };
        assert_eq!(value, i + 1, "Counter value mismatch");
    }

    // The storage reads should be the same for all transactions
    let key_reads: Vec<_> = storage_reads
        .iter()
        .filter(|item| {
            item.column == Column::ContractsState.id() && item.key == storage_slot_key
        })
        .map(|item| item.value.clone().unwrap())
        .collect();

    for storage_bytes in key_reads.iter() {
        assert_eq!(
            0,
            counter_contract::value_from_storage_bytes(&storage_bytes),
            "Counter value from storage events mismatch"
        );
    }
}
