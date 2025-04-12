// Tests related to the predicate execution feature
#![allow(unused_imports)]

use crate::helpers::TestSetupBuilder;
use fuel_core_storage::tables::Coins;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        field::{
            ChargeableBody,
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

fn predicate_checking_predicate_data_matches_input_data_coin() -> Vec<u8> {
    let len_reg = 0x13;
    let res_reg = 0x10;
    let input_index = 0;
    let expected_data_mem_location = 0x22;
    let actual_data_mem_location = 0x23;
    vec![
        op::gtf_args(len_reg, input_index, GTFArgs::InputDataCoinDataLength),
        // get expected data from predicate data
        op::gtf_args(
            expected_data_mem_location,
            input_index,
            GTFArgs::InputCoinPredicateData,
        ),
        // get actual data
        op::gtf_args(
            actual_data_mem_location,
            input_index,
            GTFArgs::InputDataCoinData,
        ),
        // compare
        op::meq(
            res_reg,
            expected_data_mem_location,
            actual_data_mem_location,
            len_reg,
        ),
        op::ret(res_reg),
    ]
    .into_iter()
    .collect()
}

#[tokio::test]
async fn submit__tx_with_predicate_can_check_data_coin() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    let mut rng = StdRng::seed_from_u64(2322);

    // given
    let amount = 500;
    let limit = 1000;
    let asset_id = rng.gen();
    let predicate = predicate_checking_predicate_data_matches_input_data_coin();
    let coin_data = vec![123; 100];
    let predicate_data = coin_data.clone();
    let owner = Input::predicate_owner(&predicate);
    let mut predicate_tx =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::data_coin_predicate(
                rng.gen(),
                owner,
                amount,
                asset_id,
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
                coin_data,
            ))
            .add_output(Output::change(rng.gen(), 0, asset_id))
            .script_gas_limit(limit)
            .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

    use fuel_core_storage::StorageAsRef;

    let coin = context
        .srv
        .shared
        .database
        .on_chain()
        .storage::<Coins>()
        .get(&predicate_tx.inputs()[0].utxo_id().unwrap())
        .expect("Failed to get coin from db");

    tracing::debug!("zzzzzz {:?}", coin);

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

    // when
    let predicate_tx = predicate_tx.into();
    context
        .client
        .submit_and_await_commit(&predicate_tx)
        .await
        .unwrap();

    // then
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

#[allow(unused_variables)]
fn predicate_checking_output_data_matches_input_data_coin() -> Vec<u8> {
    let output_data_len_reg = 0x13;
    let input_data_len_reg = 0x14;
    let res_reg = 0x10;
    let lengths_match_reg = 0x11;
    let data_match_reg = 0x12;
    let input_index = 0;
    let output_index = 0;
    let output_data_mem_location = 0x22;
    let input_data_mem_location = 0x23;
    vec![
        op::gtf_args(
            input_data_len_reg,
            input_index,
            GTFArgs::InputDataCoinDataLength,
        ),
        op::gtf_args(
            output_data_len_reg,
            output_index,
            GTFArgs::OutputDataCoinDataLength,
        ),
        // output data location
        op::gtf_args(
            output_data_mem_location,
            output_index,
            GTFArgs::OutputDataCoinData,
        ),
        // input data location
        op::gtf_args(
            input_data_mem_location,
            input_index,
            GTFArgs::InputDataCoinData,
        ),
        // compare
        op::eq(lengths_match_reg, input_data_len_reg, output_data_len_reg),
        op::meq(
            data_match_reg,
            input_data_mem_location,
            output_data_mem_location,
            output_data_len_reg,
        ),
        op::and(res_reg, lengths_match_reg, data_match_reg),
        op::ret(res_reg),
    ]
    .into_iter()
    .collect()
}

#[tokio::test]
async fn submit__tx_with_predicate_can_check_input_and_output_data_coins() {
    let mut rng = StdRng::seed_from_u64(2322);

    // given
    let input_amount = 500;
    let output_amount = 200;
    let change_amount = input_amount - output_amount;
    let limit = 1000;
    let asset_id = rng.gen();
    let predicate = predicate_checking_output_data_matches_input_data_coin();
    let coin_data = vec![123; 100];
    let predicate_data = vec![1, 2, 3, 4, 5];
    let owner = Input::predicate_owner(&predicate);
    let mut predicate_tx =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::data_coin_predicate(
                rng.gen(),
                owner,
                input_amount,
                asset_id,
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
                coin_data.clone(),
            ))
            .add_output(Output::data_coin(
                rng.gen(),
                output_amount,
                asset_id,
                coin_data,
            ))
            .add_output(Output::change(rng.gen(), 0, asset_id))
            .script_gas_limit(limit)
            .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

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

    // when
    let predicate_tx = predicate_tx.into();
    context
        .client
        .submit_and_await_commit(&predicate_tx)
        .await
        .unwrap();

    // then
    let transaction: Transaction = context
        .client
        .transaction(&predicate_tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction
        .try_into()
        .unwrap();

    if let Output::DataCoin { amount, .. } = transaction.as_script().unwrap().outputs()[0]
    {
        assert!(
            amount == output_amount,
            "Expected output amount to be {}, but got {}",
            amount,
            output_amount
        );
    } else {
        panic!("Expected output data coin");
    }

    if let Output::Change { amount, .. } = transaction.as_script().unwrap().outputs()[1] {
        assert!(
            amount == change_amount,
            "Expected change amount to be {}, but got {}",
            change_amount,
            amount
        );
    } else {
        panic!("Expected change output");
    }
}

#[tokio::test]
async fn submit__tx_with_predicate_can_check_read_only_input_predicate_data_coin_and_output_data_coins(
) {
    let mut rng = StdRng::seed_from_u64(2322);

    // given
    let input_amount = 500;
    let output_amount = 200;
    let change_amount = input_amount - output_amount;
    let limit = 1000;
    let asset_id = rng.gen();
    let predicate = predicate_checking_output_data_matches_input_data_coin();
    let coin_data = vec![123; 100];
    let predicate_data = vec![1, 2, 3, 4, 5];
    let true_predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let true_predicate_owner = Input::predicate_owner(&true_predicate);
    let owner = Input::predicate_owner(&predicate);
    let mut predicate_tx =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::read_only_data_coin_predicate(
                rng.gen(),
                true_predicate_owner,
                123,
                asset_id,
                Default::default(),
                Default::default(),
                true_predicate,
                vec![],
                coin_data.clone(),
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                input_amount,
                asset_id,
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
            ))
            .add_output(Output::data_coin(
                rng.gen(),
                output_amount,
                asset_id,
                coin_data,
            ))
            .add_output(Output::change(rng.gen(), 0, asset_id))
            .script_gas_limit(limit)
            .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder::default()
        .config_coin_inputs_from_transactions(&[&predicate_tx])
        .finalize()
        .await;

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

    let chain_id = context.srv.shared.config.chain_id();

    // when
    let predicate_tx = predicate_tx.into();
    let _status = context
        .client
        .submit_and_await_commit(&predicate_tx)
        .await
        .unwrap();

    // then
    let id = predicate_tx.id(&chain_id);
    let transaction: Transaction = context
        .client
        .transaction(&id)
        .await
        .unwrap()
        .unwrap()
        .transaction
        .try_into()
        .unwrap();

    if let Output::DataCoin { amount, .. } = transaction.as_script().unwrap().outputs()[0]
    {
        assert!(
            amount == output_amount,
            "Expected output amount to be {}, but got {}",
            amount,
            output_amount
        );
    } else {
        panic!("Expected output data coin");
    }

    if let Output::Change { amount, .. } = transaction.as_script().unwrap().outputs()[1] {
        assert!(
            amount == change_amount,
            "Expected change amount to be {}, but got {}",
            change_amount,
            amount
        );
    } else {
        panic!("Expected change output");
    }
}
