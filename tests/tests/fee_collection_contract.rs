#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::arithmetic_side_effects)]

use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

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
    fuel_crypto::SecretKey,
    fuel_tx::{
        Cacheable,
        Contract,
        Finalizable,
        Input,
        Output,
        TransactionBuilder,
        Witness,
    },
    fuel_types::{
        canonical::Serialize,
        Address,
        AssetId,
        BlockHeight,
        ChainId,
        ContractId,
        Salt,
    },
};

struct TestContext {
    address: Address,
    contract_id: ContractId,
    _node: FuelService,
    client: FuelClient,
}

const AMOUNT: u64 = 1000;
const TIP: u64 = AMOUNT / 2;

async fn setup(rng: &mut StdRng) -> TestContext {
    // Make contract that coinbase fees are collected into
    let address: Address = rng.gen();
    let salt: Salt = rng.gen();
    let contract = fuel_core::chain_config::fee_collection_contract::generate(address);
    let witness: Witness = contract.clone().into();
    let contract = Contract::from(contract);
    let root = contract.root();
    let state_root = Contract::default_state_root();
    let contract_id = contract.id(&salt, &root, &state_root);
    let mut create_tx = TransactionBuilder::create(witness.clone(), salt, vec![])
        .add_random_fee_input()
        .add_output(Output::contract_created(contract_id, state_root))
        .finalize();
    create_tx
        .precompute(&ChainId::default())
        .expect("tx should be valid");
    let contract_id = create_tx.metadata().as_ref().unwrap().body.contract_id;

    // Start up a node
    let mut config = Config::local_node();
    config.debug = true;
    config.block_producer.coinbase_recipient = Some(contract_id);
    let node = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(node.bound_address);

    // Submit contract creation tx
    let tx_status = client
        .submit_and_await_commit(&create_tx.into())
        .await
        .unwrap();
    assert!(matches!(tx_status, TransactionStatus::Success { .. }));
    let bh = client.produce_blocks(1, None).await.unwrap();
    assert_eq!(bh, BlockHeight::new(2));

    // No fees should have been collected yet
    let contract_balance = client.contract_balance(&(contract_id), None).await.unwrap();
    assert_eq!(contract_balance, 0);

    TestContext {
        address,
        contract_id,
        _node: node,
        client,
    }
}

/// This makes a block with a single transaction that has a fee,
/// so that the coinbase fee is collected into the contract
async fn make_block_with_fee(rng: &mut StdRng, ctx: &TestContext) {
    let old_balance = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();

    // Run a script that does nothing, but will cause fee collection
    let tx =
        TransactionBuilder::script([op::ret(RegId::ONE)].into_iter().collect(), vec![])
            .max_fee_limit(AMOUNT)
            .tip(TIP)
            .add_unsigned_coin_input(
                SecretKey::random(rng),
                rng.gen(),
                AMOUNT,
                Default::default(),
                Default::default(),
            )
            .script_gas_limit(1_000_000)
            .finalize_as_transaction();
    let tx_status = ctx.client.submit_and_await_commit(&tx).await.unwrap();
    assert!(matches!(tx_status, TransactionStatus::Success { .. }));

    // Now the coinbase fee should be reflected in the contract balance
    let new_balance = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert!(new_balance > old_balance);
}

async fn collect_fees(ctx: &TestContext) {
    let TestContext {
        client,
        contract_id,
        ..
    } = ctx;

    let asset_id = AssetId::BASE;
    let output_index = 1u64;
    let call_struct_register = 0x10;
    // Now call the fee collection contract to withdraw the fees
    let script = vec![
        // Point to the call structure
        op::gtf_args(call_struct_register, 0x00, GTFArgs::ScriptData),
        op::addi(
            call_struct_register,
            call_struct_register,
            (asset_id.size() + output_index.size()) as u16,
        ),
        op::call(call_struct_register, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::ONE),
    ];

    let tx = TransactionBuilder::script(
            script.into_iter().collect(),asset_id.to_bytes().into_iter()
                .chain(output_index.to_bytes().into_iter())
                .chain(contract_id
                    .to_bytes().into_iter())
                .chain(0u64.to_bytes().into_iter())
                .chain(0u64.to_bytes().into_iter())
                .collect(),
        )
            .add_random_fee_input() // No coinbase fee for this block
            .script_gas_limit(1_000_000)
            .add_input(Input::contract(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                *contract_id,
            ))
            .add_output(Output::contract(1, Default::default(), Default::default()))
            .add_output(Output::variable(
                Default::default(),
                Default::default(),
                Default::default(),
            ))
            .finalize_as_transaction();

    let tx_status = client.submit_and_await_commit(&tx).await.unwrap();
    assert!(
        matches!(tx_status, TransactionStatus::Success { .. }),
        "{tx_status:?}"
    );
}

#[tokio::test]
async fn happy_path() {
    let rng = &mut StdRng::seed_from_u64(0);

    let ctx = setup(rng).await;

    for _ in 0..10 {
        make_block_with_fee(rng, &ctx).await;
    }

    // When
    // Before withdrawal, the recipient's balance should be zero,
    // and the contract balance should be non-zero.
    let contract_balance_before_collect = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert_ne!(contract_balance_before_collect, 0);
    assert_eq!(ctx.client.balance(&ctx.address, None).await.unwrap(), 0);

    // When
    collect_fees(&ctx).await;

    // Then

    // Make sure that the full balance was been withdrawn
    let contract_balance_after_collect = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert_eq!(contract_balance_after_collect, 0);

    // Make sure that the full balance was been withdrawn
    assert_eq!(
        ctx.client.balance(&ctx.address, None).await.unwrap(),
        contract_balance_before_collect
    );
}

/// Attempts fee collection when no balance has accumulated yet
#[tokio::test]
async fn no_fees_collected_yet() {
    let rng = &mut StdRng::seed_from_u64(0);

    let ctx = setup(rng).await;

    // Given
    let contract_balance_before_collect = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert_eq!(contract_balance_before_collect, 0);
    assert_eq!(ctx.client.balance(&ctx.address, None).await.unwrap(), 0);

    // When
    collect_fees(&ctx).await;

    // Then

    // Make sure that the balance is still zero
    let contract_balance = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert_eq!(contract_balance, 0);

    // There were no coins to withdraw
    assert_eq!(ctx.client.balance(&ctx.address, None).await.unwrap(), 0);
}

#[tokio::test]
async fn missing_variable_output() {
    let rng = &mut StdRng::seed_from_u64(0);

    let ctx = setup(rng).await;
    make_block_with_fee(rng, &ctx).await;

    let asset_id = AssetId::BASE;
    let output_index = 1u64;
    let call_struct_register = 0x10;

    // Now call the fee collection contract to withdraw the fees,
    // but unlike in the happy path, we don't add the variable output to the transaction.
    let script = vec![
        // Point to the call structure
        op::gtf_args(call_struct_register, 0x00, GTFArgs::ScriptData),
        op::addi(
            call_struct_register,
            call_struct_register,
            (asset_id.size() + output_index.size()) as u16,
        ),
        op::call(call_struct_register, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::ONE),
    ];
    let tx = TransactionBuilder::script(
            script.into_iter().collect(),
            asset_id.to_bytes().into_iter()
                .chain(output_index.to_bytes().into_iter())
                .chain(ctx.contract_id
                    .to_bytes().into_iter())
                .chain(0u64.to_bytes().into_iter())
                .chain(0u64.to_bytes().into_iter())
                .collect(),
        )
            .add_random_fee_input() // No coinbase fee for this block
            .script_gas_limit(1_000_000)
            .add_input(Input::contract(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                ctx.contract_id,
            ))
            .add_output(Output::contract(1, Default::default(), Default::default()))
            .finalize_as_transaction();

    let tx_status = ctx.client.submit_and_await_commit(&tx).await.unwrap();
    let TransactionStatus::Failure { reason, .. } = tx_status else {
        panic!("Expected failure");
    };
    assert_eq!(reason, "OutputNotFound");

    // Make sure that nothing was withdrawn
    let contract_balance = ctx
        .client
        .contract_balance(&ctx.contract_id, None)
        .await
        .unwrap();
    assert_eq!(contract_balance, TIP);
    let asset_balance = ctx.client.balance(&ctx.address, None).await.unwrap();
    assert_eq!(asset_balance, 0);
}
