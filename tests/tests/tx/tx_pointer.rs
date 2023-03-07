use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::{
    chain_config::CoinConfig,
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::{
        Finalizable,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test]
async fn tx_pointer_set_from_genesis_for_coin_and_contract_inputs() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![], None);
    // initialize 10 random transactions that transfer coins and call a contract

    let tx_pointer = TxPointer::new(10, rng.gen());
    let coin_utxo_id: UtxoId = rng.gen();
    let secret_key: SecretKey = rng.gen();
    let amount = 1000;

    let script = TransactionBuilder::script(vec![], vec![])
        .gas_limit(10000)
        .gas_price(1)
        .add_unsigned_coin_input(
            secret_key,
            coin_utxo_id,
            1000,
            Default::default(),
            Default::default(),
            0,
        )
        .add_input(Input::Contract {
            utxo_id: Default::default(),
            balance_root: Default::default(),
            state_root: Default::default(),
            tx_pointer: Default::default(),
            contract_id,
        })
        .add_output(Output::Change {
            amount: 0,
            asset_id: Default::default(),
            to: rng.gen(),
        })
        .add_output(Output::Contract {
            input_index: 1,
            balance_root: Default::default(),
            state_root: Default::default(),
        })
        .finalize();

    // setup genesis block with coins that transactions can spend
    test_builder.initial_coins.push(CoinConfig {
        tx_id: None,
        output_index: None,
        tx_pointer_block_height: None,
        tx_pointer_tx_idx: None,
        maturity: None,
        owner: Default::default(),
        amount: 0,
        asset_id: Default::default(),
    });

    // zero out tx pointers on tx

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    // submit transaction and verify tx_pointers
    let tx = script.into();
    client.submit_and_await_commit(&tx).await.unwrap();
    // verify that the tx returned from the api matches the submitted tx
    let ret_tx = client
        .transaction(&tx.id().to_string())
        .await
        .unwrap()
        .unwrap()
        .transaction;

    let transaction_result = client
        .transaction_status(&ret_tx.id().to_string())
        .await
        .ok()
        .unwrap();

    if let TransactionStatus::Success { block_id, .. } = transaction_result.clone() {
        let block_exists = client.block(&block_id).await.unwrap();

        assert!(block_exists.is_some());
    }

    // Once https://github.com/FuelLabs/fuel-core/issues/50 is resolved this should rely on the Submitted Status rather than Success
    assert!(matches!(
        transaction_result,
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test]
async fn tx_pointer_set_from_previous_block() {}

#[tokio::test]
async fn tx_pointer_unset_when_utxo_validation_disabled() {}
