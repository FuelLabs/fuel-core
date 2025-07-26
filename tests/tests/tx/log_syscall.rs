use crate::helpers::TestSetupBuilder;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
};
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};
use std::array;

#[tokio::test]
async fn syscall_logs_produced_by_both_predicates_and_script() {
    const SYSCALL_LOG: u64 = 1000;
    const LOG_FD_STDOUT: u64 = 1;

    let mut rng = StdRng::seed_from_u64(121210);

    const AMOUNT: u64 = 1_000;
    const NUM_PREDICATES: usize = 4;

    let text_reg = 0x10;
    let text_size_reg = 0x11;
    let syscall_id_reg = 0x12;
    let fd_reg = 0x13;
    let tmp_reg = 0x14;

    let predicates: [_; NUM_PREDICATES] = array::from_fn(|i| i).map(|i| {
        let predicate_data: Vec<u8> =
            format!("Hello from Predicate {i}!").bytes().collect();
        let predicate = vec![
            op::movi(syscall_id_reg, SYSCALL_LOG as u32),
            op::movi(fd_reg, LOG_FD_STDOUT as u32),
            op::gm_args(tmp_reg, GMArgs::GetVerifyingPredicate),
            op::gtf_args(text_reg, tmp_reg, GTFArgs::InputCoinPredicateData),
            op::movi(text_size_reg, predicate_data.len().try_into().unwrap()),
            op::ecal(syscall_id_reg, fd_reg, text_reg, text_size_reg),
            op::ret(RegId::ONE),
        ]
        .into_iter()
        .collect();
        let predicate_owner = Input::predicate_owner(&predicate);
        (predicate, predicate_data, predicate_owner)
    });

    let script_data: Vec<u8> = "Hello from Script!".bytes().collect();
    let script: Vec<u8> = vec![
        op::movi(syscall_id_reg, SYSCALL_LOG as u32),
        op::movi(fd_reg, LOG_FD_STDOUT as u32),
        op::gtf_args(text_reg, 0x00, GTFArgs::ScriptData),
        op::movi(text_size_reg, script_data.len().try_into().unwrap()),
        op::ecal(syscall_id_reg, fd_reg, text_reg, text_size_reg),
        op::ret(RegId::ONE),
    ]
    .into_iter()
    .collect();

    // Given
    let mut tx = TransactionBuilder::script(script, script_data);
    tx.script_gas_limit(1_000_000);
    for (predicate, predicate_data, predicate_owner) in predicates {
        tx.add_input(Input::coin_predicate(
            rng.r#gen(),
            predicate_owner,
            AMOUNT,
            AssetId::BASE,
            Default::default(),
            Default::default(),
            predicate,
            predicate_data,
        ));
    }
    let tx = tx.finalize();

    let mut context = TestSetupBuilder::default();
    context.utxo_validation = true;
    context.config_coin_inputs_from_transactions(&[&tx]);
    let context = context.finalize().await;

    let mut tx = tx.into();
    context.client.estimate_predicates(&mut tx).await.unwrap();

    // When
    let result = context.client.submit_and_await_commit(&tx).await;

    // Then
    result.expect("Transaction should be executed successfully");
}

#[tokio::test]
async fn syscall_logs_allow_special_characters() {
    const SYSCALL_LOG: u64 = 1000;
    const LOG_FD_STDOUT: u64 = 1;

    let mut rng = StdRng::seed_from_u64(121210);

    const AMOUNT: u64 = 1_000;

    let text_reg = 0x10;
    let text_size_reg = 0x11;
    let syscall_id_reg = 0x12;
    let fd_reg = 0x13;

    let predicate_data: Vec<u8> = "Special\nCharacters:€π≈!".bytes().collect();
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
    let tx = TransactionBuilder::script(Default::default(), Default::default())
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
    context.config_coin_inputs_from_transactions(&[&tx]);
    let context = context.finalize().await;

    let mut tx = tx.into();
    context.client.estimate_predicates(&mut tx).await.unwrap();

    // When
    let result = context.client.submit_and_await_commit(&tx).await;

    // Then
    result.expect("Transaction should be executed successfully");
}
