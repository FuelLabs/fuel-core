// Tests related to the predicate execution feature

use crate::helpers::TestSetupBuilder;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
        },
        *,
    },
    fuel_types::ChainId,
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test]
async fn transaction_with_valid_predicate_is_executed() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let limit = 1000;
    let asset_id = rng.gen();
    // make predicate return 1 which mean valid
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let mut predicate_tx =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                amount,
                asset_id,
                Default::default(),
                Default::default(),
                predicate,
                vec![],
            ))
            .add_output(Output::change(rng.gen(), 0, asset_id))
            .script_gas_limit(limit)
            .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

    assert_eq!(predicate_tx.inputs()[0].predicate_gas_used().unwrap(), 0);

    predicate_tx
        .estimate_predicates(
            &CheckPredicateParams::from(
                &context
                    .srv
                    .shared
                    .config
                    .snapshot_reader
                    .chain_config()
                    .consensus_parameters,
            ),
            MemoryInstance::new(),
        )
        .expect("Predicate check failed");

    assert_ne!(predicate_tx.inputs()[0].predicate_gas_used().unwrap(), 0);

    let predicate_tx = predicate_tx.into();
    context
        .client
        .submit_and_await_commit(&predicate_tx)
        .await
        .unwrap();

    // check transaction change amount to see if predicate was spent
    let transaction = context
        .client
        .transaction(&predicate_tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;

    assert!(
        matches!(transaction.as_script().unwrap().outputs()[0], Output::Change { amount: change_amount, .. } if change_amount == amount)
    )
}

#[tokio::test]
async fn transaction_with_invalid_predicate_is_rejected() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let asset_id = rng.gen();
    // make predicate return 0 which means invalid
    let predicate = op::ret(RegId::ZERO).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            amount,
            asset_id,
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

    let result = context.client.submit(&predicate_tx.into()).await;

    assert!(result.is_err())
}

#[tokio::test]
async fn transaction_with_predicates_that_exhaust_gas_limit_are_rejected() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let asset_id = rng.gen();
    // make predicate jump in infinite loop
    let predicate = op::jmp(RegId::ZERO).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            amount,
            asset_id,
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

    let err = context
        .client
        .submit(&predicate_tx.into())
        .await
        .expect_err("expected tx to fail");

    assert!(
        err.to_string()
            .contains("PredicateVerificationFailed(OutOfGas)"),
        "got unexpected error {err}"
    )
}
