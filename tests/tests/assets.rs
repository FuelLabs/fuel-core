use fuel_core::service::Config;
use fuel_core_bin::FuelService;
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
        ContractIdExt,
        Input,
        Output,
        TransactionBuilder,
        TxPointer,
        UtxoId,
        Witness,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::{
        Call,
        Contract,
        Salt,
    },
};

#[tokio::test]
async fn asset_info_mint_burn() {
    // Constants
    let mint_amount: u32 = 100;
    let burn_amount: u32 = 50;
    let gas_limit = 1_000_000;

    // setup server & client
    let config = Config::local_node();
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    // Register ids
    let reg_len: u8 = 0x10;
    let reg_mint_amount: u8 = 0x11;
    let reg_burn_amount: u8 = 0x12;
    let reg_jump_cond: u8 = 0x13;
    let mut ops = vec![
        // Allocate space for sub asset id
        op::movi(reg_len, 32),
        op::aloc(reg_len),
        // Put the sub id 0 in memory
        op::sb(RegId::HP, 0, 0),
    ];
    // Set the mint amount in a register
    ops.extend([op::movi(reg_mint_amount, mint_amount)]);
    // Set the burn amount in a register
    ops.extend([op::movi(reg_burn_amount, burn_amount)]);
    ops.extend(vec![
        // Read the register filled by the script data to either make a mint or burn
        // If 0, mint, if 2, burn
        op::jmpf(reg_jump_cond, 0),
        op::mint(reg_mint_amount, RegId::HP),
        op::ret(RegId::ONE),
        op::burn(reg_burn_amount, RegId::HP),
        op::ret(RegId::ONE),
    ]);
    // Contract code.
    let bytecode: Witness = ops.into_iter().collect::<Vec<u8>>().into();

    // Setup the contract.
    let salt = Salt::zeroed();
    let contract = Contract::from(bytecode.as_ref());
    let root = contract.root();
    let state_root = Contract::initial_state_root(std::iter::empty());
    let contract_id = contract.id(&salt, &root, &state_root);
    let output = Output::contract_created(contract_id, state_root);

    // Create the contract deploy transaction.
    let contract_deploy = TransactionBuilder::create(bytecode, salt, vec![])
        .add_fee_input()
        .add_output(output)
        .finalize_as_transaction();
    // Deploy the contract.
    matches!(
        client.submit_and_await_commit(&contract_deploy).await,
        Ok(TransactionStatus::Success { .. })
    );

    let script_ops = vec![
        // Place 0 in the jump condition register to trigger the mint
        op::movi(reg_jump_cond, 0),
        // Call the contract that handle the asset and will mint
        op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::ONE),
    ];
    let script_data: Vec<u8> = [Call::new(contract_id, 0, 0).to_bytes().as_slice()]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let script = TransactionBuilder::script(
        script_ops.into_iter().collect::<Vec<u8>>().into(),
        script_data,
    )
    // Add contract as input of the transaction
    .add_input(Input::contract(
        UtxoId::new(Bytes32::zeroed(), 0),
        Bytes32::zeroed(),
        state_root,
        TxPointer::default(),
        contract_id,
    ))
    .script_gas_limit(gas_limit)
    .add_fee_input()
    // Add contract as output of the transaction
    .add_output(Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed()))
    .finalize_as_transaction();

    // Submit and await commit of the mint transaction
    let status = client.submit_and_await_commit(&script).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    // Query asset info before burn
    let initial_supply = client
        .asset_info(&contract_id.asset_id(&Bytes32::zeroed()))
        .await
        .unwrap()
        .total_supply;

    // Then
    // We should have the minted amount first
    assert_eq!(initial_supply, mint_amount as u128);

    // Create and submit transaction that burns coins
    let script_ops = vec![
        // Place 2 in the jump condition register to trigger the burn
        op::movi(reg_jump_cond, 2),
        // Call the contract that handle the asset and will burn
        op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::ONE),
    ];
    let script_data: Vec<u8> = [Call::new(contract_id, 0, 0).to_bytes().as_slice()]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let script = TransactionBuilder::script(
        script_ops.into_iter().collect::<Vec<u8>>().into(),
        script_data,
    )
    // Add contract as input of the transaction
    .add_input(Input::contract(
        UtxoId::new(Bytes32::zeroed(), 0),
        Bytes32::zeroed(),
        state_root,
        TxPointer::default(),
        contract_id,
    ))
    .script_gas_limit(gas_limit)
    .add_fee_input()
    // Add contract as output of the transaction
    .add_output(Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed()))
    .finalize_as_transaction();

    let status = client.submit_and_await_commit(&script).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    // Query asset info after burn
    let final_supply = client
        .asset_info(&contract_id.asset_id(&Bytes32::zeroed()))
        .await
        .unwrap()
        .total_supply;

    // Then
    // We should have the minted amount reduced by the burned amount
    assert_eq!(final_supply, (mint_amount - burn_amount) as u128);
}
