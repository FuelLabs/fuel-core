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
        predicate::EmptyStorage,
    },
};
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};

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
        TransactionBuilder::script(Default::default(), Default::default())
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

#[tokio::test]
async fn transaction_with_invalid_predicate_is_rejected() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let amount = 500;
    let asset_id = rng.r#gen();
    // make predicate return 0 which means invalid
    let predicate = op::ret(RegId::ZERO).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
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
    let asset_id = rng.r#gen();
    // make predicate jump in infinite loop
    let predicate = op::jmp(RegId::ZERO).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
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
            .contains("PredicateVerificationFailed(OutOfGas { index: 0 })"),
        "got unexpected error {err}"
    )
}

#[tokio::test]
async fn syscall_is_allowed_in_the_predicate() {
    const SYSCALL_LOG: u64 = 1000;
    const LOG_FD_STDOUT: u64 = 1;

    let mut rng = StdRng::seed_from_u64(121210);

    const AMOUNT: u64 = 1_000;

    let test_input = "Hello, \n LogSyscall!";
    let predicate_data: Vec<u8> = test_input.bytes().collect();

    let text_reg = 0x10;
    let text_size_reg = 0x11;
    let syscall_id_reg = 0x12;
    let fd_reg = 0x13;

    let predicate = vec![
        op::movi(syscall_id_reg, SYSCALL_LOG as u32),
        op::movi(fd_reg, LOG_FD_STDOUT as u32),
        op::gtf_args(text_reg, 0x00, GTFArgs::InputCoinPredicateData),
        op::movi(text_size_reg, predicate_data.len().try_into().unwrap()),
        op::ecal(syscall_id_reg, fd_reg, text_reg, text_size_reg),
        op::ret(RegId::ONE),
    ]
    .into_iter()
    .collect();
    let predicate_owner = Input::predicate_owner(&predicate);

    // Given
    let transaction_with_predicate =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::coin_predicate(
                rng.r#gen(),
                predicate_owner,
                AMOUNT,
                AssetId::BASE,
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
            ))
            .finalize();

    let mut context = TestSetupBuilder::default();
    context.utxo_validation = true;
    context.config_coin_inputs_from_transactions(&[&transaction_with_predicate]);
    let context = context.finalize().await;

    let mut transaction_with_predicates = transaction_with_predicate.into();
    context
        .client
        .estimate_predicates(&mut transaction_with_predicates)
        .await
        .unwrap();

    dbg!(&transaction_with_predicates);

    // When
    let result = context
        .client
        .submit_and_await_commit(&transaction_with_predicates)
        .await;

    // Then
    result.expect("Transaction should be executed successfully");
}
