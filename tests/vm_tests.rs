#![allow(warnings)]

mod common;

use crate::common::setup;
use fuel_vm_rust::bit_funcs::*;
use fuel_vm_rust::consts::{MemWord, FUEL_MAX_MEMORY_SIZE};
use fuel_vm_rust::interpreter::*;
use fuel_vm_rust::opcodes::*;
use std::thread;

fn local_vm_setup() {
    let mut vm: VM = VM::new();
    let p: &mut Program = &mut vm.program;

    let o1 = Opcode::Add(0, 1, 1);
    let o2 = Opcode::Stop();

    p.code.push(o1.ser());
    p.code.push(o2.ser());

    vm.run();

    assert_eq!(vm.get_register_value(1), 3);
    assert_eq!(vm.get_register_value(0), 6);

    vm.dump_registers();
}

#[test]
pub fn test_vm_start() {
    println!("Hello, world!");

    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(local_vm_setup)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

#[test]
pub fn test_u32_bits() {
    assert_eq!(from_u32_to_u8_recurse(3u32, 32 - 8, 8), 3);
    assert_eq!(from_u32_to_u8_recurse(3u32, 0, 32), 3);

    // currently does not exclude invalid left and len values, returns v
    assert_eq!(from_u32_to_u8_recurse(3u32, 0, 33), 3);

    assert_eq!(from_u32_to_u8_recurse(3, 32 - 2, 1), 1);

    assert_eq!(from_u32_to_u8_recurse(3, 0, 33), 3);
}

#[test]
pub fn test_u64_bits() {
    assert_eq!(from_u64_to_u8(3u64, 64 - 8, 8), 3);
    assert_eq!(from_u64_to_u8(3u64, 0, 64), 3);

    // currently does not exclude invalid left and len values, returns v
    assert_eq!(from_u64_to_u8(3u64, 0, 65), 3);

    assert_eq!(from_u64_to_u8(3, 64 - 2, 1), 1);

    assert_eq!(from_u64_to_u8_recurse(3, 0, 65), 3);
}

#[test]
pub fn test_set_u64_bits() {
    let v = 0u64;
    assert!(set_bits_in_u64(v, 1, 0, 8) > 1)
}

#[test]
pub fn test_program_create() {
    println!("test_program_create()");

    let mut p = Program::new();
    let o1 = Opcode::Add(1, 1, 1);
    let o2 = Opcode::Stop();

    p.code.push(o1.ser());
    p.code.push(o2.ser());

    let mut line = 1;
    for c in p.code {
        println!("line {}: {:#b}", line, c);
        let oo: Opcode = Opcode::deser(c);
        match oo {
            Opcode::Add(rd, rs, rt) => {
                assert_eq!(line, 1);
                println!("Add({},{},{})", rd, rs, rt)
            }
            Opcode::Sub(rd, rs, rt) => {
                println!("Sub({},{},{})", rd, rs, rt)
            }
            Opcode::Halt(rs) => {
                assert_eq!(line, 2);
                println!("Halt({})", rs)
            }
            _ => println!("Oops"),
        }
        line += 1;
    }
    println!("Done");
}

#[test]
fn test_stack_new() {
    let vm: VM = VM::new();
    let v1 = vm.get_registers();
    let mut s: CallFrame = CallFrame::new();
    &s.copy_registers(vm.get_registers().clone());
    assert_eq!(s.prev_registers, v1);
}

fn setup_program_w_pushpop() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    p.code.push(Opcode::Push(0).ser());
    p.code.push(Opcode::Call(2, 1).ser());
    p.code.push(Opcode::Add(0, 1, 1).ser());
    p.code.push(Opcode::Ret(4).ser());
    p.code.push(Opcode::Stop().ser());

    vm.run();

    assert_eq!(vm.get_register_value(1), 3);
    assert_eq!(vm.get_register_value(0), 6);

    vm.dump_registers();
}

#[test]
pub fn test_program_w_pushpop() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_pushpop)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn setup_program_w_call() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;
    p.code.push(Opcode::Add(0, 1, 1).ser());
    // p.code.push(Opcode::Push(1).ser());
    // p.code.push(Opcode::Call(3, 1).ser());
    // p.code.push(Opcode::Add(0, 1, 1).ser());
    // p.code.push(Opcode::Ret().ser());

    vm.run();

    assert_eq!(vm.get_register_value(1), 3);
    assert_eq!(vm.get_register_value(0), 6);

    vm.dump_registers();
}

#[test]
pub fn test_program_w_call() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_call)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn setup_program_w_tx() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_abi(p);

    let mut tx: Transactionleaf = Transactionleaf::new_default();
    tx.metadata[0] = 1;

    handle_txleaf(tx, &mut vm);

    vm.dump_registers();
}

fn build_program_for_abi(p: &mut Program) {
    // function selector from tx
    // retrieve meta
    p.code.push(Opcode::Pop(0).ser());

    // contract ABI
    p.code.push(Opcode::SetI(1, 0).ser());
    p.code.push(Opcode::SetI(2, 1).ser());

    // function selector
    p.code.push(Opcode::Beq(0, 1, 6).ser());
    p.code.push(Opcode::Beq(0, 2, 8).ser());
    p.code.push(Opcode::J(5).ser());

    // func
    p.code.push(Opcode::Add(3, 3, 3).ser());
    p.code.push(Opcode::J(3).ser());
    // func
    p.code.push(Opcode::Add(3, 3, 3).ser());
    p.code.push(Opcode::Add(3, 3, 3).ser());

    p.code.push(Opcode::Stop().ser());
}

#[test]
pub fn test_program_w_tx() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_tx)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn setup_program_w_new_tx_format() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_abi_new_tx(p);

    let mut tx: Ftx = Ftx::default();
    let data_vec = transform_from_u32_to_u8(&p.code);
    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: data_vec.len() as u16,
        data: data_vec,
    };
    tx.inputs = vec![tx_input];

    handle_ftx(tx, &mut vm);

    vm.dump_registers();
}

fn build_program_for_abi_new_tx(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::Pop(0).ser());

    // function ABI
    // 1
    p.code.push(Opcode::SetI(1, 0).ser());
    // 2
    p.code.push(Opcode::SetI(2, 1).ser());

    // match function selector to available ABI functions
    // 3
    p.code.push(Opcode::Beq(0, 1, 6).ser());
    // 4
    p.code.push(Opcode::Beq(0, 2, 8).ser());
    // 5
    p.code.push(Opcode::J(5).ser());

    // 6
    p.code.push(Opcode::Add(3, 3, 3).ser());
    // 7
    p.code.push(Opcode::J(3).ser());
    // 8
    p.code.push(Opcode::Add(3, 3, 3).ser());
    // 9
    p.code.push(Opcode::Add(3, 3, 3).ser());
    // 10
    p.code.push(Opcode::Stop().ser());
}

#[test]
pub fn test_program_w_new_tx_format() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_new_tx_format)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

#[test]
pub fn test_program_w_new_tx_format_extended() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_new_tx_format_extended)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn setup_program_w_new_tx_format_extended() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;
    build_program_for_abi_new_tx(p);
    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx1_output = FOutput {
        output_type: FOutputTypeEnum::FOutputContract {
            input_index: 0,
            amount_witness_index: 0,
            state_witness_index: 0,
        },
        data: vec![0],
    };
    tx1.outputs = vec![tx1_output];

    let mut tx2: Ftx = Ftx::default();

    let mut tx_ops: Vec<u32> = Vec::new();
    &tx_ops.push(Opcode::SetI(0, 0).ser());
    &tx_ops.push(Opcode::Push(0).ser());

    let mut tx_data: Vec<u8> = transform_from_u32_to_u8(&tx_ops);

    let mut tx_input2 = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [0; 32],
        }),
        data_length: tx_data.len() as u16,
        data: tx_data,
    };
    tx2.inputs = vec![tx_input2];

    handle_ftx(tx1, &mut vm);

    vm.set_state(1);
    handle_ftx(tx2, &mut vm);

    vm.dump_registers();
}

fn build_program_for_keccak(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::SetI(0, 6).ser());
    p.code.push(Opcode::SetI(1, 1).ser());
    p.code.push(Opcode::SetI(2, 2).ser());
    p.code.push(Opcode::SetI(3, 3).ser());

    p.code.push(Opcode::Sw(0, 1, 0).ser());
    p.code.push(Opcode::Sw(0, 2, 1).ser());
    p.code.push(Opcode::Sw(0, 3, 2).ser());

    p.code.push(Opcode::Keccak(0, 1, 2).ser());

    p.code.push(Opcode::Push(0).ser());

    // 10
    p.code.push(Opcode::Stop().ser());
}

fn setup_program_w_keccak() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_keccak(p);

    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: 0,
        data: Vec::new(),
    };
    tx1.inputs = vec![tx_input];

    handle_ftx(tx1, &mut vm);

    vm.dump_registers();
}

#[test]
pub fn test_program_w_keccak() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_keccak)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn build_program_for_sha256(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::SetI(0, 6).ser());
    p.code.push(Opcode::SetI(1, 1).ser());
    p.code.push(Opcode::SetI(2, 2).ser());
    p.code.push(Opcode::SetI(3, 3).ser());

    p.code.push(Opcode::Sw(0, 1, 0).ser());
    p.code.push(Opcode::Sw(0, 2, 1).ser());
    p.code.push(Opcode::Sw(0, 3, 2).ser());

    p.code.push(Opcode::Sha256(0, 1, 2).ser());

    p.code.push(Opcode::Push(0).ser());

    // 10
    p.code.push(Opcode::Stop().ser());
}

fn setup_program_w_sha256() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_sha256(p);

    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: 0,
        data: Vec::new(),
    };
    tx1.inputs = vec![tx_input];

    handle_ftx(tx1, &mut vm);

    vm.dump_registers();
}

#[test]
pub fn test_program_w_sha256() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_sha256)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn build_program_for_ecrecover(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::SetI(0, 6).ser());
    p.code.push(Opcode::SetI(1, 1).ser());
    p.code.push(Opcode::SetI(2, 2).ser());
    p.code.push(Opcode::SetI(3, 3).ser());

    p.code.push(Opcode::Sw(0, 1, 0).ser());
    p.code.push(Opcode::Sw(0, 2, 1).ser());
    p.code.push(Opcode::Sw(0, 3, 2).ser());

    p.code.push(Opcode::Ecrecover(0, 1, 2).ser());

    p.code.push(Opcode::Push(0).ser());

    // 10
    p.code.push(Opcode::Stop().ser());
}

fn setup_program_w_ecrecover() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_ecrecover(p);

    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: 0,
        data: Vec::new(),
    };
    tx1.inputs = vec![tx_input];

    handle_ftx(tx1, &mut vm);

    vm.dump_registers();
}

#[test]
pub fn test_program_w_ecrecover() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_ecrecover)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn build_program_for_storage(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::SetI(0, 6).ser());
    p.code.push(Opcode::SetI(1, 1).ser());
    p.code.push(Opcode::SetI(2, 2).ser());
    p.code.push(Opcode::SetI(3, 3).ser());

    p.code.push(Opcode::Swwx(4, 2).ser());
    p.code.push(Opcode::Srwx(0, 4).ser());
    // p.code.push(Opcode::Swwx(4, 1).ser());
    // p.code.push(Opcode::Srwx(0, 4).ser());

    p.code.push(Opcode::Push(0).ser());

    // 10
    p.code.push(Opcode::Stop().ser());
}

fn setup_program_w_storage() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_storage(p);

    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: 0,
        data: Vec::new(),
    };
    tx1.inputs = vec![tx_input];

    handle_ftx(tx1, &mut vm);

    vm.dump_registers();

    println!("keyvalue store dump");
    for (k, v) in vm.store.iter() {
        println!("k: {}, v: {}", k, v);
    }
}

#[test]
pub fn test_program_w_storage() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_storage)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

fn build_program_for_mem(p: &mut Program) {
    // function selector from tx

    // 0
    p.code.push(Opcode::SetI(0, 6).ser());
    p.code.push(Opcode::SetI(1, 1).ser());
    p.code.push(Opcode::SetI(2, 2).ser());
    p.code.push(Opcode::SetI(3, 3).ser());

    p.code.push(Opcode::MemCp(0, 1, 1).ser());
    p.code.push(Opcode::MemEq(0, 1, 1, 1).ser());

    p.code.push(Opcode::Push(0).ser());

    // 10
    p.code.push(Opcode::Stop().ser());
}

fn setup_program_w_mem() {
    let mut vm: VM = VM::new();

    let p: &mut Program = &mut vm.program;

    build_program_for_mem(p);

    let data_vec = transform_from_u32_to_u8(&p.code);

    let mut tx1: Ftx = Ftx::default();
    tx1.script_length = data_vec.len() as u16;
    tx1.script = data_vec;

    let mut tx_input = FInput {
        utxo_id: [0; 32],
        input_type: FInputTypeEnum::Contract(FInputContract {
            contract_id: [1; 32],
        }),
        data_length: 0,
        data: Vec::new(),
    };
    tx1.inputs = vec![tx_input];

    handle_ftx(tx1, &mut vm);

    vm.dump_registers();

    println!("keyvalue store dump");
    for (k, v) in vm.store.iter() {
        println!("k: {}, v: {}", k, v);
    }
}

#[test]
pub fn test_program_w_mem() {
    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(setup_program_w_mem)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}

#[test]
pub fn test_op_add() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Add(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);

    vm.set_register_value(0, MemWord::MAX);
    vm.execute(Opcode::Add(2, 0, 1));
    assert_eq!(vm.get_of(), 8);
}

#[test]
pub fn test_op_addi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::Addi(2, 0, 1));

    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);

    vm.set_register_value(0, MemWord::MAX);
    vm.execute(Opcode::Addi(2, 0, 1));
    assert_eq!(vm.get_of(), 8);
}

#[test]
pub fn test_op_and() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::And(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 1);
    vm.set_register_value(1, 3);
    vm.execute(Opcode::And(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_andi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::Andi(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 1);
    vm.execute(Opcode::Andi(2, 0, 3));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_cpsr() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::CopyRegister(1, 0));
    let i: MemWord = vm.get_register_value(1);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_div() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 4);
    vm.set_register_value(1, 2);
    vm.execute(Opcode::Div(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);

    vm.set_register_value(0, 4);
    vm.set_register_value(1, 3);
    vm.execute(Opcode::Div(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);

    assert_eq!(i, 1);
    assert_eq!(vm.hi, 1);
}

#[test]
pub fn test_op_divi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 4);
    vm.execute(Opcode::Divi(2, 0, 2));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);

    vm.set_register_value(0, 4);
    vm.execute(Opcode::Divi(2, 0, 3));
    let i: MemWord = vm.get_register_value(2);

    assert_eq!(i, 1);
    assert_eq!(vm.hi, 1);
}

#[test]
pub fn test_op_eq() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Eq(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(1, 0);
    vm.execute(Opcode::Eq(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);
}

#[test]
pub fn test_op_exp() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 0);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Exp(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 1);
    vm.execute(Opcode::Eq(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 2);
    vm.set_register_value(1, 2);
    vm.execute(Opcode::Exp(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 4);
}

#[test]
pub fn test_op_expi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 0);
    vm.execute(Opcode::Expi(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 1);
    vm.execute(Opcode::Expi(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 2);
    vm.execute(Opcode::Expi(2, 0, 2));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 4);
}

#[test]
pub fn test_op_gt() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Gt(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(1, 0);
    vm.execute(Opcode::Gt(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_mod() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 4);
    vm.set_register_value(1, 2);
    vm.execute(Opcode::Mod(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 4);
    vm.set_register_value(1, 3);
    vm.execute(Opcode::Mod(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_modi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 4);
    vm.execute(Opcode::Modi(2, 0, 2));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.execute(Opcode::Modi(2, 0, 3));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_mult() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.set_register_value(1, 2);
    vm.execute(Opcode::Mult(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 4);

    vm.set_register_value(0, 4);
    vm.set_register_value(1, 3);
    vm.execute(Opcode::Mult(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 12);
}

#[test]
pub fn test_op_multi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.execute(Opcode::Multi(2, 0, 2));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 4);

    vm.execute(Opcode::Multi(2, 0, 3));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 6);
}

#[test]
pub fn test_op_or() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 0);
    vm.set_register_value(1, 0);
    vm.execute(Opcode::Or(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 0);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Or(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 1);
    vm.set_register_value(1, 2);
    vm.execute(Opcode::Or(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 3);
}

#[test]
pub fn test_op_ori() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 0);
    vm.execute(Opcode::Ori(2, 0, 0));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 0);
    vm.execute(Opcode::Ori(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);

    vm.set_register_value(0, 1);
    vm.execute(Opcode::Ori(2, 0, 2));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 3);
}

#[test]
pub fn test_op_sll() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::Sll(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);
}

#[test]
pub fn test_op_sllv() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Sllv(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 2);
}

#[test]
pub fn test_op_srl() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.execute(Opcode::Srl(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_srlv() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Srlv(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
}

#[test]
pub fn test_op_sub() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Sub(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
    assert_eq!(vm.get_of(), 7);

    vm.set_register_value(0, 2);
    vm.set_register_value(1, 3);
    vm.execute(Opcode::Sub(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, u64::MAX);
    assert_eq!(vm.get_of(), 8);
}

#[test]
pub fn test_op_subi() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 2);
    vm.execute(Opcode::Subi(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 1);
    assert_eq!(vm.get_of(), 7);

    vm.set_register_value(0, 2);
    vm.execute(Opcode::Subi(2, 0, 3));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, u64::MAX);
    assert_eq!(vm.get_of(), 8);
}

#[test]
pub fn test_op_xor() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Xor(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 2);
    vm.set_register_value(1, 1);
    vm.execute(Opcode::Xor(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 3);
}

#[test]
pub fn test_op_xori() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::Xori(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 0);

    vm.set_register_value(0, 2);
    vm.execute(Opcode::Xori(2, 0, 1));
    let i: MemWord = vm.get_register_value(2);
    assert_eq!(i, 3);
}

#[test]
pub fn test_op_j() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 1);
    vm.execute(Opcode::J(3));
    assert_eq!(vm.get_top_frame().unwrap().pc, 3);
}

#[test]
pub fn test_op_jr() {
    let mut vm: VM = VM::new();
    vm.set_register_value(0, 3);
    vm.execute(Opcode::Jr(0));
    assert_eq!(vm.get_top_frame().unwrap().pc, 3);
}

#[test]
pub fn test_op_jnz() {
    let mut vm: VM = VM::new();
    vm.execute(Opcode::Jnz(0, 1));
    assert_eq!(vm.get_top_frame().unwrap().pc, 3);
}

#[test]
pub fn test_op_jnzi() {
    let mut vm: VM = VM::new();
    vm.execute(Opcode::Jnzi(0, 1));
    assert_eq!(vm.get_top_frame().unwrap().pc, 3);

    vm.execute(Opcode::Jnzi(0, 0));
    assert_eq!(vm.get_top_frame().unwrap().pc, 4);
}
