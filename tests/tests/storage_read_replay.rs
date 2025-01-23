use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_tx::{
        Bytes32,
        ContractId,
        CreateMetadata,
        Finalizable,
        Input,
        Output,
        StorageSlot,
        TransactionBuilder,
    },
    fuel_types::BlockHeight,
    fuel_vm::{
        Salt,
        SecretKey,
    },
};
use futures::StreamExt;
use rand::{
    Rng,
    SeedableRng,
};

async fn make_counter_contract(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
) -> (ContractId, BlockHeight) {
    let maturity = Default::default();

    let code: Vec<_> = [
        // Make zero key
        op::movi(0x12, 32),
        op::aloc(0x12),
        // Read value
        op::srw(0x10, 0x11, 0x12),
        // Increment value
        op::addi(0x10, 0x10, 1),
        // Write value
        op::sww(0x12, 0x11, 0x10),
        // Return new counter value
        op::ret(0x10),
    ]
    .into_iter()
    .collect();

    let salt: Salt = rng.gen();
    let tx = TransactionBuilder::create(
        code.into(),
        salt,
        vec![StorageSlot::new(Bytes32::zeroed(), Bytes32::zeroed())],
    )
    .maturity(maturity)
    .add_fee_input()
    .add_contract_created()
    .finalize();

    let contract_id = CreateMetadata::compute(&tx).unwrap().contract_id;

    let mut status_stream = client.submit_and_await_status(&tx.into()).await.unwrap();
    let intermediate_status = status_stream.next().await.unwrap().unwrap();
    assert!(matches!(
        intermediate_status,
        TransactionStatus::Submitted { .. }
    ));
    let final_status = status_stream.next().await.unwrap().unwrap();
    let TransactionStatus::Success { block_height, .. } = final_status else {
        panic!("Tx wasn't included in a block: {:?}", final_status);
    };
    (contract_id, block_height)
}

async fn increment_counter(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
) -> BlockHeight {
    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::log(0x10, RegId::ZERO, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];

    let mut script_data = contract_id.to_vec();
    script_data.extend(0u64.to_be_bytes());
    script_data.extend(0u64.to_be_bytes());

    let tx = TransactionBuilder::script(script.into_iter().collect(), script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.gen(),
            u32::MAX as u64,
            Default::default(),
            Default::default(),
        )
        .add_input(Input::contract(
            rng.gen(),
            rng.gen(),
            rng.gen(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut status_stream = client.submit_and_await_status(&tx).await.unwrap();
    let intermediate_status = status_stream.next().await.unwrap().unwrap();
    assert!(matches!(
        intermediate_status,
        TransactionStatus::Submitted { .. }
    ));
    let final_status = status_stream.next().await.unwrap().unwrap();
    let TransactionStatus::Success { block_height, .. } = final_status else {
        panic!("Tx wasn't included in a block: {:?}", final_status);
    };
    block_height
}

/// Create a counter contract.
/// Increment it multiple times, and make sure the replay gives correct storage state every time.
#[tokio::test(flavor = "multi_thread")]
async fn test_storage_read_replay_returns_counter_state() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);

    let mut node_config = Config::local_node();
    node_config.debug = true;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let (contract_id, block_height) = make_counter_contract(&client, &mut rng).await;

    let _replay = client
        .storage_read_replay(&block_height)
        .await
        .expect("Failed to replay storage read");

    let mut storage_slot_key = contract_id.to_vec();
    storage_slot_key.extend(Bytes32::zeroed().to_vec());

    for i in 0..10u64 {
        let block_height = increment_counter(&client, &mut rng, contract_id).await;

        let replay = client
            .storage_read_replay(&block_height)
            .await
            .expect("Failed to replay storage read");

        let value = replay
            .iter()
            .find(|item| item.column == "ContractsState" && item.key == storage_slot_key)
            .expect("No storage read found")
            .value
            .clone();

        let mut expected_value = [0; 32];
        expected_value[..8].copy_from_slice(&i.to_be_bytes());

        assert!(value == Some(expected_value.to_vec()));
    }
}
