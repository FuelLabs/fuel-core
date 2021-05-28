use super::common::*;
use super::*;

use std::mem;

const CONTRACT_ADDRESS_SIZE: usize = mem::size_of::<ContractAddress>();

#[test]
fn mint_burn() {
    let mut balance = 1000;

    let mut vm = Interpreter::default();

    let gas_price = 10;
    let gas_limit = 1_000_000;
    let maturity = 100;

    let mint = deploy_contract(
        gas_price,
        gas_limit,
        maturity,
        &mut vm,
        &[
            Opcode::ADDI(0x10, REG_FP, CallFrame::inputs_outputs_offset() as Immediate12),
            Opcode::LW(0x11, 0x10, 1),
            Opcode::LW(0x10, 0x10, 0),
            Opcode::JNEI(0x10, REG_ZERO, 6),
            Opcode::MINT(0x11),
            Opcode::JI(7),
            Opcode::BURN(0x11),
            Opcode::RET(0x11),
        ],
    );

    let mut script_ops = vec![
        Opcode::ADDI(0x10, REG_ZERO, 0x00),
        Opcode::ADDI(0x11, 0x10, CONTRACT_ADDRESS_SIZE as Immediate12),
        Opcode::CALL(0x10, REG_ZERO, 0x10, 0x10),
        Opcode::RET(0x30),
    ];

    let script = program_to_bytes(script_ops.as_slice());
    let input = Input::contract(d(), d(), d(), mint);
    let output = Output::contract(0, d(), d());

    // Mint balance
    let mut script_data = mint.to_vec();
    script_data.extend((0 as Word).to_be_bytes());
    script_data.extend((2 as Word).to_be_bytes());
    script_data.extend((0 as Word).to_be_bytes()); // Flag for mint subroutine
    script_data.extend((balance as Word).to_be_bytes());

    let mut tx = Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script.clone(),
        script_data,
        vec![input.clone()],
        vec![output],
        vec![],
    );

    let script_data_mem = Interpreter::tx_mem_address() + tx.script_data_offset().unwrap();
    script_ops[0] = Opcode::ADDI(0x10, REG_ZERO, script_data_mem as Immediate12);
    let script_mem = program_to_bytes(script_ops.as_slice());

    match &mut tx {
        Transaction::Script { script, .. } => script.as_mut_slice().copy_from_slice(script_mem.as_slice()),
        _ => unreachable!(),
    }

    assert_eq!(0, vm.color_balance(&mint));
    vm.init(tx).expect("Failed to init VM with tx create!");
    vm.run().expect("Failed to execute contract!");
    assert_eq!(balance as Word, vm.color_balance(&mint));

    // Try to burn more than balance
    let mut script_data = mint.to_vec();
    script_data.extend((0 as Word).to_be_bytes());
    script_data.extend((2 as Word).to_be_bytes());
    script_data.extend((1 as Word).to_be_bytes()); // Flag for burn subroutine
    script_data.extend((balance as Word + 1).to_be_bytes());

    let mut tx = Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script.clone(),
        script_data,
        vec![input.clone()],
        vec![output],
        vec![],
    );

    let script_data_mem = Interpreter::tx_mem_address() + tx.script_data_offset().unwrap();
    script_ops[0] = Opcode::ADDI(0x10, REG_ZERO, script_data_mem as Immediate12);
    let script_mem = program_to_bytes(script_ops.as_slice());

    match &mut tx {
        Transaction::Script { script, .. } => script.as_mut_slice().copy_from_slice(script_mem.as_slice()),
        _ => unreachable!(),
    }

    assert_eq!(balance, vm.color_balance(&mint));
    vm.init(tx).expect("Failed to init VM with tx create!");
    assert!(vm.run().is_err());
    assert_eq!(balance as Word, vm.color_balance(&mint));

    // Burn some of the balance
    let burn = 100;

    let mut script_data = mint.to_vec();
    script_data.extend((0 as Word).to_be_bytes());
    script_data.extend((2 as Word).to_be_bytes());
    script_data.extend((1 as Word).to_be_bytes()); // Flag for burn subroutine
    script_data.extend((burn as Word).to_be_bytes());

    let mut tx = Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script.clone(),
        script_data,
        vec![input.clone()],
        vec![output],
        vec![],
    );

    let script_data_mem = Interpreter::tx_mem_address() + tx.script_data_offset().unwrap();
    script_ops[0] = Opcode::ADDI(0x10, REG_ZERO, script_data_mem as Immediate12);
    let script_mem = program_to_bytes(script_ops.as_slice());

    match &mut tx {
        Transaction::Script { script, .. } => script.as_mut_slice().copy_from_slice(script_mem.as_slice()),
        _ => unreachable!(),
    }

    assert_eq!(balance, vm.color_balance(&mint));
    vm.init(tx).expect("Failed to init VM with tx create!");
    vm.run().expect("Failed to execute contract!");
    balance -= burn;
    assert_eq!(balance as Word, vm.color_balance(&mint));

    // Burn the remainder balance
    let mut script_data = mint.to_vec();
    script_data.extend((0 as Word).to_be_bytes());
    script_data.extend((2 as Word).to_be_bytes());
    script_data.extend((1 as Word).to_be_bytes()); // Flag for burn subroutine
    script_data.extend((balance as Word).to_be_bytes());

    let mut tx = Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script.clone(),
        script_data,
        vec![input.clone()],
        vec![output],
        vec![],
    );

    let script_data_mem = Interpreter::tx_mem_address() + tx.script_data_offset().unwrap();
    script_ops[0] = Opcode::ADDI(0x10, REG_ZERO, script_data_mem as Immediate12);
    let script_mem = program_to_bytes(script_ops.as_slice());

    match &mut tx {
        Transaction::Script { script, .. } => script.as_mut_slice().copy_from_slice(script_mem.as_slice()),
        _ => unreachable!(),
    }

    assert_eq!(balance, vm.color_balance(&mint));
    vm.init(tx).expect("Failed to init VM with tx create!");
    vm.run().expect("Failed to execute contract!");
    assert_eq!(0, vm.color_balance(&mint));
}
