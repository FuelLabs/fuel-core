// Tests related to the predicate execution feature

use crate::helpers::TestSetupBuilder;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        field::Outputs,
        *,
    },
    fuel_vm::consts::*,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test]
async fn transaction_with_predicates_is_rejected_when_feature_disabled() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let asset_id = rng.gen();
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            rng.gen(),
            500,
            asset_id,
            Default::default(),
            0,
            rng.gen::<[u8; 32]>().to_vec(),
            rng.gen::<[u8; 32]>().to_vec(),
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder {
        predicates: false,
        ..Default::default()
    }
    .config_coin_inputs_from_transactions(&[&predicate_tx])
    .finalize()
    .await;

    let result = context.client.submit(&predicate_tx.into()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn transaction_with_predicate_is_executed_when_feature_enabled() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let asset_id = rng.gen();
    // make predicate return 1 which mean valid
    let predicate = Opcode::RET(REG_ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            amount,
            asset_id,
            Default::default(),
            0,
            predicate,
            vec![],
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder {
        predicates: true,
        ..Default::default()
    }
    .config_coin_inputs_from_transactions(&[&predicate_tx])
    .finalize()
    .await;

    let predicate_tx = predicate_tx.into();
    context
        .client
        .submit_and_await_commit(&predicate_tx)
        .await
        .unwrap();

    // check transaction change amount to see if predicate was spent
    let transaction = context
        .client
        .transaction(&predicate_tx.id().to_string())
        .await
        .unwrap()
        .unwrap()
        .transaction;

    assert!(
        matches!(transaction.as_script().unwrap().outputs()[0], Output::Change { amount: change_amount, .. } if change_amount == amount)
    )
}

#[tokio::test]
async fn transaction_with_invalid_predicate_is_rejected_when_feature_is_enabled() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let asset_id = rng.gen();
    // make predicate return 0 which means invalid
    let predicate = Opcode::RET(REG_ZERO).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            amount,
            asset_id,
            Default::default(),
            0,
            predicate,
            vec![],
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder {
        predicates: true,
        ..Default::default()
    }
    .config_coin_inputs_from_transactions(&[&predicate_tx])
    .finalize()
    .await;

    let result = context.client.submit(&predicate_tx.into()).await;

    assert!(result.is_err())
}
