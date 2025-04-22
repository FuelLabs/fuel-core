//! Tests related to vm memory usage

use crate::helpers::TestSetupBuilder;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        *,
    },
    fuel_types::ChainId,
};


#[tokio::test]
async fn transaction_that_uses_heap_does_not_leave_dirty_memory_for_next_transaction() {
    // This test checks that a transaction that uses the heap does not leave dirty memory for the next transaction.
    // It does this by allocating memory on the heap, storing data on the heap, and then returning from the script.
    // The next transaction should not see any dirty memory from the previous transaction.

    // setup script tx
    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let base_reg = 0x10;
    let word_reg = 0x11;
    let heap_size = 10_000;
    let store_count = 100;

    fn iter_sw(count: usize, base_reg: u8, word_reg: u8) -> Vec<Instruction> {
        let mut instructions = Vec::with_capacity(count * 3);
        for _ in 0..count {
            instructions.push(op::move_(base_reg, RegId::HP));
            instructions.push(op::movi(word_reg, 1));
            instructions.push(op::sw(base_reg, word_reg, 1));
        }

        instructions
    }

    // allocate on the heap
    let pre_instructions = vec![
        op::movi(base_reg, heap_size),
        op::aloc(base_reg),
    ];

    // store data on heap
    let store_instructions = iter_sw(store_count, base_reg, word_reg);

    // return from script
    let post_instructions = vec![op::ret(RegId::ZERO)];

    let mut script = Vec::new();
    script.extend(pre_instructions);
    script.extend(store_instructions);
    script.extend(post_instructions);
    let script = script.into_iter().collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    let mut context = TestSetupBuilder::default();
    context.utxo_validation = false;
    let context = context.finalize()
        .await;

    context.client.submit_and_await_commit(&tx).await.unwrap();

    let tx: Transaction = context.client
        .transaction(&tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction
        .try_into()
        .unwrap();

    let tx_status = context.client.transaction_status(&tx.id(&ChainId::default())).await.unwrap();

    assert!(matches!(tx_status, fuel_core_client::client::types::TransactionStatus::Success { .. }));


    fn iter_lw(count: usize, base_reg: u8, word_reg: u8) -> Vec<Instruction> {
        let mut instructions = Vec::with_capacity(count * 5);
        for _ in 0..count {
            instructions.push(op::move_(base_reg, RegId::HP));
            instructions.push(op::lw(word_reg, base_reg, 0));
            // if word_reg != 0, then rvrt
            instructions.push(op::jnzi(word_reg, 2)); // jump to revert
            instructions.push(op::ret(word_reg)); // return if zero
            instructions.push(op::rvrt(word_reg)); // revert with word_reg
        }

        instructions
    }

    // allocate on the heap
    let pre_instructions = vec![
        op::movi(base_reg, heap_size),
        op::aloc(base_reg),
    ];

    // read data from the heap, and if any non-zero value is detected, then revert
    let load_instructions = iter_lw(store_count, base_reg, word_reg);

    let mut script = Vec::new();
    script.extend(pre_instructions);
    script.extend(load_instructions);
    let script = script.into_iter().collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    let mut context = TestSetupBuilder::default();
    context.utxo_validation = false;
    let context = context.finalize()
        .await;

    context.client.submit_and_await_commit(&tx).await.unwrap();

    let tx: Transaction = context.client
        .transaction(&tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction
        .try_into()
        .unwrap();

    let tx_status = context.client.transaction_status(&tx.id(&ChainId::default())).await.unwrap();

    assert!(matches!(tx_status, fuel_core_client::client::types::TransactionStatus::Success { .. }));
}
