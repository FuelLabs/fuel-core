//! A simple contract that mints requested subtokens.

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
        Finalizable,
        Input,
        Output,
        Receipt,
        StorageSlot,
        SubAssetId,
        Transaction,
        TransactionBuilder,
        field::Outputs,
    },
    fuel_types::{
        BlockHeight,
        bytes::WORD_SIZE,
        canonical::Serialize,
    },
    fuel_vm::{
        Call,
        SecretKey,
    },
};
use rand::Rng;

// Registers used to pass arguments to the contract
const REG_SUB_ASSET_ID_PTR: RegId = RegId::new(0x10);
const REG_AMOUNT: RegId = RegId::new(0x11);

/// Deploy the contract
pub async fn deploy(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
) -> (BlockHeight, ContractId) {
    let chain_info = client.chain_info().await.expect("failed to get chain info");
    let base_asset_id = chain_info.consensus_parameters.base_asset_id();

    let code: Vec<_> = [
        op::mint(REG_AMOUNT, REG_SUB_ASSET_ID_PTR),
        op::ret(RegId::ONE),
    ]
    .into_iter()
    .collect();

    let tx = TransactionBuilder::create(
        code.into(),
        rng.r#gen(),
        vec![StorageSlot::new(Bytes32::zeroed(), Bytes32::zeroed())],
    )
    .maturity(Default::default())
    .add_unsigned_coin_input(
        SecretKey::random(rng),
        rng.r#gen(),
        u32::MAX as u64,
        *base_asset_id,
        Default::default(),
    )
    .add_contract_created()
    .finalize();
    let contract_id = *tx.outputs()[0].contract_id().unwrap();
    let Ok(TransactionStatus::Success { block_height, .. }) =
        client.submit_and_await_commit(&tx.into()).await
    else {
        panic!("Failed to deploy contract");
    };
    (block_height, contract_id)
}

/// Mint a new subtoken with given sub asset id and amount
pub fn mint_tx(
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
    sub_asset_id: SubAssetId,
    amount: u64,
) -> Transaction {
    let script = [
        op::gtf_args(0x20, RegId::ZERO, GTFArgs::ScriptData),
        op::addi(REG_SUB_ASSET_ID_PTR, 0x20, Call::LEN as _),
        op::lw(
            REG_AMOUNT,
            REG_SUB_ASSET_ID_PTR,
            (Bytes32::LEN / WORD_SIZE) as _,
        ),
        op::call(0x20, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::RET),
    ];

    let mut script_data = contract_id.to_vec();
    script_data.extend(0u64.to_be_bytes());
    script_data.extend(0u64.to_be_bytes());
    script_data.extend(sub_asset_id.to_bytes());
    script_data.extend(amount.to_be_bytes());

    TransactionBuilder::script(script.into_iter().collect(), script_data)
        .script_gas_limit(1_000_000)
        .maturity(Default::default())
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

/// Mint a new subtoken with given sub asset id and amount
pub async fn mint(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
    sub_asset_id: SubAssetId,
    amount: u64,
) -> BlockHeight {
    let tx = mint_tx(rng, contract_id, sub_asset_id, amount);

    let Ok(TransactionStatus::Success {
        block_height,
        receipts,
        ..
    }) = client.submit_and_await_commit(&tx).await
    else {
        panic!("Tx wasn't included in a block");
    };

    // Self-test: ensure mint receipt is correct
    let Some(Receipt::Mint {
        contract_id: r_contract_id,
        sub_id,
        ..
    }) = receipts.get(1)
    else {
        panic!("Expected mint receipt");
    };
    assert_eq!(contract_id, *r_contract_id);
    assert_eq!(sub_asset_id, *sub_id);

    block_height
}
