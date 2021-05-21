use super::common::r;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

use std::io::{Read, Write};

#[test]
fn call() {
    let mut vm = Interpreter::default();

    let id = r();
    let color: Color = r();
    let input = Input::contract(r(), r(), r(), id);
    let output = Output::contract(0, r(), r());

    let code = vec![0xcd; 256];
    let mut code_data = vec![];
    code_data.extend(&color);

    let alloc = 512;
    let mut buffer = vec![0u8; 1024];
    let mut call = Call::new(
        id,
        vec![(VM_MAX_RAM - alloc..VM_MAX_RAM).into()],
        vec![(VM_MAX_RAM - alloc..VM_MAX_RAM).into()],
    );
    let n = call.read(buffer.as_mut_slice()).expect("Failed to serialize call!");
    code_data.extend(&buffer[..n]);

    let tx = Transaction::script(
        1,
        1000000,
        100,
        code.clone(),
        code_data.clone(),
        vec![input],
        vec![output],
        vec![],
    );
    vm.init(tx).expect("Failed to init VM!");

    vm.execute(Opcode::ADDI(0x20, REG_ZERO, alloc as Immediate12)).unwrap();
    vm.execute(Opcode::ALOC(0x20)).unwrap();

    // Transaction script starts after 9 words
    let code_address = vm.tx_stack() + 72;
    assert_eq!(code.as_slice(), &vm.memory()[code_address..code_address + code.len()]);

    let code_data_address = vm.tx_stack() + 72 + code.len();
    assert_eq!(
        code_data.as_slice(),
        &vm.memory()[code_data_address..code_data_address + code_data.len()]
    );

    let color_address = code_data_address as Word;
    assert_eq!(
        &color,
        &vm.memory()[color_address as usize..color_address as usize + color.len()]
    );

    let call_address = color_address + color.len() as Word;
    let mut call_p = Call::new(r(), vec![], vec![]);
    call_p
        .write(&vm.memory()[call_address as usize..])
        .expect("Failed to deserialize call!");
    assert_eq!(call, call_p);

    let color_addr = 0x10;
    vm.execute(Opcode::ADDI(color_addr, REG_ZERO, color_address as Immediate12))
        .unwrap();

    let call_addr = 0x11;
    vm.execute(Opcode::ADDI(call_addr, REG_ZERO, call_address as Immediate12))
        .unwrap();

    let frame = vm.call_frame(call, color);
    vm.execute(Opcode::CALL(call_addr, REG_ZERO, color_addr, REG_ZERO))
        .unwrap();

    let mut frame_p = frame.clone();
    let fp = vm.registers()[REG_FP];
    let n = frame_p
        .write(&vm.memory()[fp as usize..])
        .expect("Failed to deserialize call frame!");
    assert_eq!(frame, frame_p);

    let ssp = fp + n as Word;
    let sp = ssp;

    assert_eq!(vm.registers()[REG_SSP], ssp);
    assert_eq!(vm.registers()[REG_SP], sp);

    let code_addr = vm.registers()[REG_PC] as usize;
    assert_eq!(code.as_slice(), &vm.memory()[code_addr..code_addr + code.len()]);
    assert_eq!(vm.registers()[REG_PC], vm.registers()[REG_IS]);

    // TODO check balances
}
