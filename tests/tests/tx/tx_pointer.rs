use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::chain_config::CoinConfig;
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
        },
        Address,
        AssetId,
        ContractId,
        Finalizable,
        Input,
        Output,
        Script,
        TransactionBuilder,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

/// Verifies that tx_pointer's are correctly set for both coin and contract input types when
/// using resources from genesis.
#[tokio::test]
async fn tx_pointer_set_from_genesis_for_coin_and_contract_inputs() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);

    let starting_block = 10.into();

    // setup genesis contract
    let contract_tx_pointer = TxPointer::new(7.into(), rng.gen());
    let (_, contract_id) =
        test_builder.setup_contract(vec![], vec![], Some(contract_tx_pointer));

    // setup genesis coin
    let coin_tx_pointer = TxPointer::new(starting_block, rng.gen());
    let coin_utxo_id: UtxoId = rng.gen();
    let secret_key: SecretKey = SecretKey::random(&mut rng);
    let owner = Input::owner(&secret_key.public_key());
    let amount = 1000;

    // add coin to genesis block
    test_builder.initial_coins.push(CoinConfig {
        tx_id: *coin_utxo_id.tx_id(),
        output_index: coin_utxo_id.output_index(),
        tx_pointer_block_height: coin_tx_pointer.block_height(),
        tx_pointer_tx_idx: coin_tx_pointer.tx_index(),
        owner,
        amount,
        asset_id: Default::default(),
    });

    // set starting block >= tx_pointer.block_height()
    test_builder.starting_block = Some(starting_block);

    // construct a transaction that uses both the coin and contract from genesis,
    // with the tx_pointer's left as null
    let script = script_tx(secret_key, amount, coin_utxo_id, contract_id, owner);

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    // submit transaction
    let tx = script.into();
    client.submit_and_await_commit(&tx).await.unwrap();

    // verify that the tx returned from the api has tx pointers set matching the genesis config
    let ret_tx = client
        .transaction(&tx.id(&Default::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;
    let ret_script = ret_tx.as_script().unwrap();

    let coin_input = &ret_script.inputs()[0];
    let contract_input = &ret_script.inputs()[1];
    // verify coin input tx_pointer is correctly set
    assert_eq!(coin_input.tx_pointer().unwrap(), &coin_tx_pointer);
    // verify contract tx_pointer is correctly set
    assert_eq!(contract_input.tx_pointer().unwrap(), &contract_tx_pointer);
}

/// Verifies tx_pointer's are correctly set for coins and contracts when consuming UTXOs
/// from a previous transaction
#[tokio::test]
async fn tx_pointer_set_from_previous_block() {
    // spend a coin + use a contract
    // make second tx using change output and same contract
    // verify tx_pointers of inputs on second tx correspond to the first transaction
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let previous_block_height = 40;

    // setup genesis contract
    let (_, contract_id) = test_builder.setup_contract(vec![], vec![], None);

    // set starting block >= tx_pointer.block_height()
    test_builder.starting_block = Some(previous_block_height.into());

    // setup genesis coin
    let coin_utxo_id: UtxoId = rng.gen();
    let secret_key: SecretKey = SecretKey::random(&mut rng);
    let owner = Input::owner(&secret_key.public_key());
    let amount = 1000;

    // setup first txn which spends UTXOs from genesis
    let tx1 = script_tx(secret_key, amount, coin_utxo_id, contract_id, owner);
    // auto-configure genesis to make tx1 work
    test_builder.config_coin_inputs_from_transactions(&[&tx1]);

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;
    let new_genesis_block_height = previous_block_height + 1;

    // submit tx1
    let tx1 = tx1.into();
    client.submit_and_await_commit(&tx1).await.unwrap();
    let next_block_height_after_genesis = new_genesis_block_height + 1;
    let ret_tx1 = client
        .transaction(&tx1.id(&Default::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;
    let ret_tx1 = ret_tx1.as_script().unwrap();

    // setup a second transaction that uses UTXOs from tx1
    let tx2 = script_tx(
        secret_key,
        ret_tx1.outputs()[0].amount().unwrap(),
        UtxoId::new(tx1.id(&Default::default()), 0),
        contract_id,
        rng.gen(),
    );
    let tx2 = tx2.into();
    client.submit_and_await_commit(&tx2).await.unwrap();

    let ret_tx2 = client
        .transaction(&tx2.id(&Default::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;

    let ret_tx2 = ret_tx2.as_script().unwrap();

    // verify coin tx_pointer is correctly set
    let expected_tx_pointer = TxPointer::new(next_block_height_after_genesis.into(), 0);
    assert_eq!(
        *ret_tx2.inputs()[0].tx_pointer().unwrap(),
        expected_tx_pointer
    );
    // verify contract tx_pointer is correctly set
    assert_eq!(
        *ret_tx2.inputs()[1].tx_pointer().unwrap(),
        expected_tx_pointer
    )
}

/// Verifies that tx_pointer is null for coins and contracts when utxo validation is disabled
#[tokio::test]
async fn tx_pointer_unset_when_utxo_validation_disabled() {
    // spend a coin + use a contract
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let block_height = 40u32;

    // setup genesis contract
    let (_, contract_id) = test_builder.setup_contract(vec![], vec![], None);

    // set starting block >= tx_pointer.block_height()
    test_builder.starting_block = Some(block_height.into());
    test_builder.utxo_validation = false;

    let secret_key: SecretKey = SecretKey::random(&mut rng);
    let script = script_tx(secret_key, 1000, Default::default(), contract_id, rng.gen());

    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let tx = script.into();
    client.submit_and_await_commit(&tx).await.unwrap();

    let ret_tx = client
        .transaction(&tx.id(&Default::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;
    let ret_tx = ret_tx.as_script().unwrap();
    // verify coin input tx_pointer is null
    assert_eq!(
        *ret_tx.inputs()[0].tx_pointer().unwrap(),
        TxPointer::default()
    );
    // verify contract input tx_pointer is null
    assert_eq!(
        *ret_tx.inputs()[1].tx_pointer().unwrap(),
        TxPointer::default()
    );
}

fn script_tx(
    secret_key: SecretKey,
    amount: u64,
    coin_utxo_id: UtxoId,
    contract_id: ContractId,
    change_owner: Address,
) -> Script {
    TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(10000)
        .add_unsigned_coin_input(
            secret_key,
            coin_utxo_id,
            amount,
            Default::default(),
            // use a zeroed out txpointer
            Default::default(),
        )
        .add_input(Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::change(change_owner, 0, AssetId::default()))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize()
}
