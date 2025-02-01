use fuel_core::{
    service::{
        Config,
        FuelService,
    },
    state::historical_rocksdb::StateRewindPolicy,
};
use fuel_core_client::client::FuelClient;
use fuel_core_poa::Trigger;
use fuel_core_types::fuel_tx::{
    field::Outputs,
    Bytes32,
    Create,
    Finalizable,
    StorageSlot,
    TransactionBuilder,
};
use futures::TryStreamExt;

#[tokio::test]
async fn all_storage_slots() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let storage_slots: Vec<StorageSlot> = (0..123)
        .map(|i| StorageSlot::new(Bytes32::new([i; 32]), Bytes32::new([i; 32])))
        .collect();
    let create: Create = TransactionBuilder::create(
        vec![].into(),
        Default::default(),
        storage_slots.clone(),
    )
    .add_contract_created()
    .add_fee_input()
    .finalize();

    // Given
    let contract_id = *create.outputs()[0].contract_id().unwrap();
    let expected_storage_slots: Vec<_> = storage_slots
        .into_iter()
        .map(|slot| (*slot.key(), slot.value().as_ref().to_vec()))
        .collect();

    // When
    client
        .submit_and_await_commit(&create.into())
        .await
        .unwrap();
    let slots_stream = client.contract_storage_slots(&contract_id).await.unwrap();

    // Then
    let actual_slots: Vec<_> = slots_stream.try_collect().await.unwrap();
    assert_eq!(expected_storage_slots.len(), actual_slots.len());
    pretty_assertions::assert_eq!(expected_storage_slots, actual_slots);
}

#[tokio::test]
async fn contract_storage_values_latest_block() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let storage_slots: Vec<StorageSlot> = (0..123)
        .map(|i| StorageSlot::new(Bytes32::new([i; 32]), Bytes32::new([i; 32])))
        .collect();
    let create: Create = TransactionBuilder::create(
        vec![].into(),
        Default::default(),
        storage_slots.clone(),
    )
    .add_contract_created()
    .add_fee_input()
    .finalize();

    // Given
    let contract_id = *create.outputs()[0].contract_id().unwrap();
    let storage_to_request: Vec<_> =
        storage_slots.iter().map(|slot| *slot.key()).collect();
    let expected_storage_slots: Vec<_> = storage_slots
        .into_iter()
        .map(|slot| (*slot.key(), slot.value().as_ref().to_vec()))
        .collect();

    // When
    client
        .submit_and_await_commit(&create.into())
        .await
        .unwrap();
    let actual_slots = client
        .contract_slots_values(&contract_id, None, storage_to_request)
        .await
        .unwrap();

    // Then
    assert_eq!(expected_storage_slots.len(), actual_slots.len());
    pretty_assertions::assert_eq!(expected_storage_slots, actual_slots);
}

#[tokio::test]
async fn contract_storage_values_block_with_create_tx() {
    let mut config = Config::local_node();
    config.block_production = Trigger::Instant;
    config.combined_db_config.state_rewind_policy = StateRewindPolicy::RewindFullRange;
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let storage_slots: Vec<StorageSlot> = (0..123)
        .map(|i| StorageSlot::new(Bytes32::new([i; 32]), Bytes32::new([i; 32])))
        .collect();
    let create: Create = TransactionBuilder::create(
        vec![].into(),
        Default::default(),
        storage_slots.clone(),
    )
    .add_contract_created()
    .add_fee_input()
    .finalize();

    // Given
    let contract_id = *create.outputs()[0].contract_id().unwrap();
    let storage_to_request: Vec<_> =
        storage_slots.iter().map(|slot| *slot.key()).collect();
    let expected_storage_slots: Vec<_> = storage_slots
        .into_iter()
        .map(|slot| (*slot.key(), slot.value().as_ref().to_vec()))
        .collect();
    let block_to_request = client.produce_blocks(1, None).await.unwrap();

    // When
    client
        .submit_and_await_commit(&create.into())
        .await
        .unwrap();
    let block_after_execution = client
        .chain_info()
        .await
        .unwrap()
        .latest_block
        .header
        .height;
    let actual_slots = client
        .contract_slots_values(
            &contract_id,
            Some(block_after_execution.into()),
            storage_to_request,
        )
        .await
        .unwrap();

    // Then
    assert_eq!(*block_to_request + 1, block_after_execution);
    assert_eq!(expected_storage_slots.len(), actual_slots.len());
    pretty_assertions::assert_eq!(expected_storage_slots, actual_slots);
}

#[tokio::test]
async fn contract_storage_values_block_before_create_tx() {
    let mut config = Config::local_node();
    config.block_production = Trigger::Instant;
    config.combined_db_config.state_rewind_policy = StateRewindPolicy::RewindFullRange;
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let storage_slots: Vec<StorageSlot> = (0..123)
        .map(|i| StorageSlot::new(Bytes32::new([i; 32]), Bytes32::new([i; 32])))
        .collect();
    let create: Create = TransactionBuilder::create(
        vec![].into(),
        Default::default(),
        storage_slots.clone(),
    )
    .add_contract_created()
    .add_fee_input()
    .finalize();

    // Given
    let contract_id = *create.outputs()[0].contract_id().unwrap();
    let storage_to_request: Vec<_> =
        storage_slots.iter().map(|slot| *slot.key()).collect();
    let expected_storage_slots: Vec<(Bytes32, Vec<u8>)> = vec![];
    let block_to_request = client.produce_blocks(1, None).await.unwrap();

    // When
    client
        .submit_and_await_commit(&create.into())
        .await
        .unwrap();
    let actual_slots = client
        .contract_slots_values(&contract_id, Some(block_to_request), storage_to_request)
        .await
        .unwrap();

    // Then
    let block_after_execution = client
        .chain_info()
        .await
        .unwrap()
        .latest_block
        .header
        .height;
    assert_eq!(*block_to_request + 1, block_after_execution);
    assert_eq!(expected_storage_slots.len(), actual_slots.len());
    pretty_assertions::assert_eq!(expected_storage_slots, actual_slots);
}
