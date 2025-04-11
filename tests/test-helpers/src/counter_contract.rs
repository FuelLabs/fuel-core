//! A simple contract that holds a counter in the storage slot zero.
//! Every time the contract is called, it increments the counter by one and returns the new value.

use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_asm::{
        GTFArgs,
        RegId,
        op,
    },
    fuel_tx::{
        Bytes32,
        ContractId,
        CreateMetadata,
        Finalizable,
        Input,
        Output,
        Receipt,
        StorageSlot,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::BlockHeight,
    fuel_vm::{
        Salt,
        SecretKey,
    },
};
use futures::StreamExt;
use rand::Rng;

/// Deploy the contract
pub async fn deploy(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
) -> (BlockHeight, ContractId) {
    let maturity = Default::default();

    let chain_info = client.chain_info().await.expect("failed to get chain info");
    let base_asset_id = chain_info.consensus_parameters.base_asset_id();

    let code: Vec<_> = [
        // Make zero key
        op::movi(0x12, 32),
        op::aloc(0x12),
        // Read value
        op::srw(0x10, 0x11, RegId::HP),
        // Increment value
        op::addi(0x10, 0x10, 1),
        // Write value
        op::sww(RegId::HP, 0x11, 0x10),
        // Return new counter value
        op::ret(0x10),
    ]
    .into_iter()
    .collect();

    let salt: Salt = rng.r#gen();
    let tx = TransactionBuilder::create(
        code.into(),
        salt,
        vec![StorageSlot::new(Bytes32::zeroed(), Bytes32::zeroed())],
    )
    .maturity(maturity)
    .add_unsigned_coin_input(
        SecretKey::random(rng),
        rng.r#gen(),
        u32::MAX as u64,
        *base_asset_id,
        Default::default(),
    )
    .add_contract_created()
    .finalize();

    let contract_id = CreateMetadata::compute(&tx).unwrap().contract_id;

    let tx = tx.into();
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
    (block_height, contract_id)
}

/// Increment the counter, returning the new value
pub fn increment_tx(
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
) -> Transaction {
    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::RET),
    ];

    let mut script_data = contract_id.to_vec();
    script_data.extend(0u64.to_be_bytes());
    script_data.extend(0u64.to_be_bytes());

    TransactionBuilder::script(script.into_iter().collect(), script_data)
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.r#gen(),
            u32::MAX as u64,
            Default::default(),
            Default::default(),
        )
        .add_input(Input::contract(
            rng.r#gen(),
            rng.r#gen(),
            rng.r#gen(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction()
}

/// Increment the counter, returning the new value
pub async fn increment(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
) -> (BlockHeight, u64) {
    let tx = increment_tx(rng, contract_id);

    let final_status = client.submit_and_await_commit(&tx).await.unwrap();
    let TransactionStatus::Success {
        block_height,
        receipts,
        ..
    } = final_status
    else {
        panic!("Tx wasn't included in a block: {:?}", final_status);
    };

    assert!(receipts.len() > 2);
    let Receipt::Return { val, .. } = receipts[receipts.len() - 2] else {
        panic!("Expected a return receipt: {:?}", receipts);
    };

    (block_height, val)
}

/// Get counter value from storage bytes
pub fn value_from_storage_bytes(storage_bytes: &[u8]) -> u64 {
    assert!(storage_bytes.len() == 32, "Storage slot size mismatch");
    assert!(
        storage_bytes[8..].iter().all(|v| *v == 0),
        "Counter values cannot be over u64::MAX"
    );
    let mut buffer = [0; 8];
    buffer.copy_from_slice(&storage_bytes[..8]);
    u64::from_be_bytes(buffer)
}
