//! Tests related to vm memory usage

use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_tx::{
        Finalizable,
        Input,
        TransactionBuilder,
    },
    fuel_types::AssetId,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use test_helpers::builder::TestSetupBuilder;

#[tokio::test]
async fn predicate_that_uses_heap_does_not_leave_dirty_memory_for_next_predicate() {
    // This test checks that a first predicate that uses the heap does not leave
    // dirty memory for the next predicate.
    // First predicate:
    // 1. Allocates memory on the heap 10_000.
    // 2. Copies data from predicate input where it is [1; 10_000]
    // 3. Compares data from the predicate input with copied data on the heap.
    //
    // Second predicate:
    // 1. Allocates memory on the heap 100_000.
    // 2. Compares data from the predicate input(it is [0; 100_000]) with the heap.
    let mut rng = StdRng::seed_from_u64(121210);

    const AMOUNT: u64 = 1_000;

    let size_reg = 0x10;
    let data_start_reg = 0x11;
    let result_reg = 0x12;
    let first_data_size = 10_000;
    let first_predicate_index = 0;

    let first_predicate = vec![
        op::movi(size_reg, first_data_size),
        op::aloc(size_reg),
        op::gtf_args(
            data_start_reg,
            first_predicate_index,
            GTFArgs::InputCoinPredicateData,
        ),
        op::mcp(RegId::HP, data_start_reg, size_reg),
        op::meq(result_reg, RegId::HP, data_start_reg, size_reg),
        op::ret(result_reg),
    ];
    let first_predicate_data = vec![1u8; first_data_size as usize];
    let first_predicate: Vec<u8> = first_predicate.into_iter().collect();
    let first_predicate_owner = Input::predicate_owner(&first_predicate);

    let second_data_size = 100_000;
    let second_predicate_index = 1;

    let second_predicate = vec![
        op::movi(size_reg, second_data_size),
        op::aloc(size_reg),
        op::gtf_args(
            data_start_reg,
            second_predicate_index,
            GTFArgs::InputCoinPredicateData,
        ),
        op::meq(result_reg, RegId::HP, data_start_reg, size_reg),
        op::ret(result_reg),
    ];
    let second_predicate_data = vec![0u8; second_data_size as usize];
    let second_predicate: Vec<u8> = second_predicate.into_iter().collect();
    let second_predicate_owner = Input::predicate_owner(&second_predicate);

    // Given
    let transaction_with_predicates =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::coin_predicate(
                rng.gen(),
                first_predicate_owner,
                AMOUNT,
                AssetId::BASE,
                Default::default(),
                Default::default(),
                first_predicate,
                first_predicate_data,
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                second_predicate_owner,
                AMOUNT,
                AssetId::BASE,
                Default::default(),
                Default::default(),
                second_predicate,
                second_predicate_data,
            ))
            .finalize();

    let mut context = TestSetupBuilder::default();
    context.utxo_validation = true;
    context.config_coin_inputs_from_transactions(&[&transaction_with_predicates]);
    let context = context.finalize().await;

    let mut transaction_with_predicates = transaction_with_predicates.into();
    context
        .client
        .estimate_predicates(&mut transaction_with_predicates)
        .await
        .unwrap();

    // When
    let result = context
        .client
        .submit_and_await_commit(&transaction_with_predicates)
        .await;

    // Then
    result.expect("Transaction should be executed successfully");
}
