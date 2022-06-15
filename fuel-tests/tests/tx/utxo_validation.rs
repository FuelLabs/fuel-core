// Tests involving utxo-validation enabled
use crate::helpers::{TestContext, TestSetupBuilder};
use fuel_core_interfaces::common::{
    fuel_tx::TransactionBuilder,
    fuel_vm::{consts::*, prelude::*},
};
use fuel_crypto::SecretKey;
use fuel_gql_client::client::types::TransactionStatus;
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};

#[tokio::test]
async fn submit_utxo_verified_tx_with_min_gas_price() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![], None);
    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .into_iter()
        .map(|i| {
            let secret = SecretKey::random(&mut rng);
            TransactionBuilder::script(
                Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .gas_limit(100)
            .gas_price(1)
            .byte_price(1)
            .add_unsigned_coin_input(rng.gen(), &secret, 1000 + i, Default::default(), 0)
            .add_input(Input::Contract {
                utxo_id: Default::default(),
                balance_root: Default::default(),
                state_root: Default::default(),
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
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions.iter().collect_vec());

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    // submit transactions and verify their status
    for tx in transactions {
        let id = client.submit(&tx).await.unwrap();
        // verify that the tx returned from the api matches the submitted tx
        let ret_tx = client
            .transaction(&id.0.to_string())
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
}

#[tokio::test]
async fn submit_utxo_verified_tx_below_min_gas_price_fails() {
    // initialize transaction
    let tx = TransactionBuilder::script(
        Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .gas_limit(100)
    .gas_price(1)
    .byte_price(1)
    .finalize();

    // initialize node with higher minimum gas price
    let mut test_builder = TestSetupBuilder::new(2322u64);
    test_builder.min_byte_price = 10;
    test_builder.min_gas_price = 10;
    let TestContext { client, .. } = test_builder.finalize().await;

    let result = client.submit(&tx).await;
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("The gas price is too low"));
}

// verify that dry run can disable utxo_validation by simulating a transaction with unsigned
// non-existent coin inputs
#[tokio::test]
async fn dry_run_override_utxo_validation() {
    let mut rng = StdRng::seed_from_u64(2322);

    let asset_id = rng.gen();
    let tx = TransactionBuilder::script(
        Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .gas_limit(1000)
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        1000,
        AssetId::default(),
        0,
        Default::default(),
    ))
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        rng.gen(),
        asset_id,
        0,
        Default::default(),
    ))
    .add_output(Output::change(rng.gen(), 0, asset_id))
    .add_witness(Default::default())
    .finalize();

    let client = TestSetupBuilder::new(2322).finalize().await.client;

    let log = client.dry_run_opt(&tx, Some(false)).await.unwrap();
    assert_eq!(2, log.len());

    assert!(matches!(log[0],
        Receipt::Return {
            val, ..
        } if val == 1));
}

// verify that dry run without utxo-validation override respects the node setting
#[tokio::test]
async fn dry_run_no_utxo_validation_override() {
    let mut rng = StdRng::seed_from_u64(2322);

    let asset_id = rng.gen();
    // construct a tx with invalid inputs
    let tx = TransactionBuilder::script(
        Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .gas_limit(1000)
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        1000,
        AssetId::default(),
        0,
        Default::default(),
    ))
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        rng.gen(),
        asset_id,
        0,
        Default::default(),
    ))
    .add_output(Output::change(rng.gen(), 0, asset_id))
    .add_witness(Default::default())
    .finalize();

    let client = TestSetupBuilder::new(2322).finalize().await.client;

    // verify that the client validated the inputs and failed the tx
    let res = client.dry_run_opt(&tx, None).await;
    assert!(res.is_err());
}
