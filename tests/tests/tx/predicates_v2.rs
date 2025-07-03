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
        predicate::EmptyStorage,
    },
};
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};

#[cfg(feature = "chargeable-tx-v2")]
#[tokio::test]
async fn transaction_with_valid_predicate_is_executed() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let limit = 1000;
    let asset_id = rng.r#gen();
    // make predicate return 1 which mean valid
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let mut predicate_tx =
        TransactionBuilder::script_v2(Default::default(), Default::default())
            .add_input(Input::coin_predicate(
                rng.r#gen(),
                owner,
                amount,
                asset_id,
                Default::default(),
                Default::default(),
                predicate,
                vec![],
            ))
            .add_output(Output::change(rng.r#gen(), 0, asset_id))
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
            &EmptyStorage,
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
    let transaction: Transaction = context
        .client
        .transaction(&predicate_tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction
        .try_into()
        .unwrap();

    assert!(
        matches!(transaction.as_script().unwrap().outputs()[0], Output::Change { amount: change_amount, .. } if change_amount == amount)
    )
}
