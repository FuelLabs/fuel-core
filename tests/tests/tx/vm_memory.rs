//! Tests related to vm memory usage

use crate::helpers::TestSetupBuilder;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
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

fn iter_sw(count: usize, base_reg: u8, word_reg: u8) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(count * 3);
    for i in 0..count {
        instructions.push(op::move_(base_reg, RegId::HP));
        instructions.push(op::movi(word_reg, 1));
        instructions.push(op::sw(base_reg, word_reg, i.try_into().unwrap()));
    }

    instructions
}

fn iter_lw(count: usize, base_reg: u8, word_reg: u8) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(count * 5);
    for i in 0..count {
        instructions.push(op::move_(base_reg, RegId::HP));
        instructions.push(op::lw(word_reg, base_reg, i.try_into().unwrap()));
        // if word_reg != 0, then rvrt
        instructions.push(op::jnzi(word_reg, 2)); // jump to revert
        instructions.push(op::ji(2)); // keep looping
        instructions.push(op::ret(RegId::ZERO)); // revert with word_reg
    }

    instructions
}

#[tokio::test]
async fn transaction_that_uses_heap_does_not_leave_dirty_memory_for_next_transaction() {
    // This test checks that a transaction that uses the heap does not leave dirty memory for the next transaction.
    // It does this by allocating memory on the heap, storing data on the heap, and then returning from the predicate.
    // The next transaction should not see any dirty memory from the previous transaction.
    let mut rng = StdRng::seed_from_u64(121210);

    let gas_limit = 100_000;
    let base_reg = 0x10;
    let word_reg = 0x11;
    let heap_size = 10_000;
    let word_count = (heap_size / 8) as usize;
    let asset_id = rng.gen();
    let amount = 500;

    // first transaction that makes memory dirty
    // allocate on the heap
    let pre_instructions = vec![op::movi(base_reg, heap_size), op::aloc(base_reg)];
    // store data on heap
    let store_instructions = iter_sw(word_count, base_reg, word_reg);
    // return from script
    let post_instructions = vec![op::ret(RegId::ONE)];

    let mut setup_predicate = Vec::new();
    setup_predicate.extend(pre_instructions);
    setup_predicate.extend(store_instructions);
    setup_predicate.extend(post_instructions);
    let setup_predicate = setup_predicate.into_iter().collect();

    let setup_predicate_owner = Input::predicate_owner(&setup_predicate);

    // second transaction that reads the heap
    // allocate on the heap
    let pre_instructions = vec![op::movi(base_reg, heap_size * 3), op::aloc(base_reg)];
    // read data from the heap, and if any non-zero value is detected, then revert
    let load_instructions = iter_lw(word_count, base_reg, word_reg);

    let mut post_predicate = Vec::new();
    post_predicate.extend(pre_instructions);
    post_predicate.extend(load_instructions);
    let post_predicate = post_predicate.into_iter().collect();
    let post_predicate_owner = Input::predicate_owner(&post_predicate);

    let mut setup_predicate_tx =
        TransactionBuilder::script(Default::default(), Default::default())
            .add_input(Input::coin_predicate(
                rng.gen(),
                setup_predicate_owner,
                amount,
                asset_id,
                Default::default(),
                Default::default(),
                setup_predicate,
                vec![],
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                post_predicate_owner,
                amount,
                asset_id,
                Default::default(),
                Default::default(),
                post_predicate,
                vec![],
            ))
            .add_output(Output::change(rng.gen(), 0, asset_id))
            .script_gas_limit(gas_limit)
            .finalize();

    let mut context = TestSetupBuilder::default();
    context.config_coin_inputs_from_transactions(&[&setup_predicate_tx]);
    let context = context.finalize().await;

    let mut memory_instance = MemoryInstance::new();

    setup_predicate_tx
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
            &mut memory_instance,
            &EmptyStorage,
        )
        .expect("Predicate check failed");

   let heap = memory_instance.heap_raw();
   let bytes = vec![0u8; heap.len()];

    let setup_predicate_tx = setup_predicate_tx.into();

    let setup_res = context
        .client
        .submit_and_await_commit(&setup_predicate_tx)
        .await;

    // the heap should be empty
    assert_eq!(bytes, heap);

    assert!(setup_res.is_err_and(|err| err
        .to_string()
        .contains("PredicateVerificationFailed(OutOfGas)")));
}
