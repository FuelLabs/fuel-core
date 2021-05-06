use super::common;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

fn alu(registers_init: &[(RegisterId, Immediate12)], op: Opcode, register: RegisterId, value: Word) {
    let vm = Interpreter::default();
    let tx = common::dummy_tx();
    let mut vm = vm.init(&tx);

    registers_init.iter().for_each(|(r, v)| {
        vm.execute(Opcode::AddI(*r, *r, *v))
            .expect("Failed to execute the provided opcode!")
    });

    vm.execute(op).expect("Failed to execute the final opcode!");
    assert_eq!(vm.registers()[register], value);
}

fn alu_err(registers_init: &[(RegisterId, Immediate12)], op: Opcode) {
    let vm = Interpreter::default();
    let tx = common::dummy_tx();
    let mut vm = vm.init(&tx);

    registers_init.iter().for_each(|(r, v)| {
        vm.execute(Opcode::AddI(*r, *r, *v))
            .expect("Failed to execute the provided opcode!")
    });

    let result = vm.execute(op);
    assert!(result.is_err());
}

#[test]
fn reserved_register() {
    alu_err(&[(0x10, 128)], Opcode::Add(REG_ZERO, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_ONE, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_OF, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_PC, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_SSP, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_SP, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_FP, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_HP, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_ERR, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_GGAS, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_CGAS, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_BAL, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_IS, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_RESERVA, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_RESERVB, 0x10, 0x11));
    alu_err(&[(0x10, 128)], Opcode::Add(REG_FLAG, 0x10, 0x11));
}

#[test]
fn add() {
    alu(&[(0x10, 128), (0x11, 25)], Opcode::Add(0x12, 0x10, 0x11), 0x12, 153);
}

#[test]
fn and() {
    alu(&[(0x10, 0xcc), (0x11, 0xaa)], Opcode::And(0x12, 0x10, 0x11), 0x12, 0x88);
    alu(&[(0x10, 0xcc)], Opcode::AndI(0x12, 0x10, 0xaa), 0x12, 0x88);
}

#[test]
fn div() {
    alu(&[(0x10, 59), (0x11, 10)], Opcode::Div(0x12, 0x10, 0x11), 0x12, 5);
    alu(&[(0x10, 59)], Opcode::DivI(0x12, 0x10, 10), 0x12, 5);
}

#[test]
fn eq() {
    alu(&[(0x10, 10), (0x11, 10)], Opcode::Eq(0x12, 0x10, 0x11), 0x12, 1);
    alu(&[(0x10, 11), (0x11, 10)], Opcode::Eq(0x12, 0x10, 0x11), 0x12, 0);
}

#[test]
fn exp() {
    alu(&[(0x10, 6), (0x11, 3)], Opcode::Exp(0x12, 0x10, 0x11), 0x12, 216);
    alu(&[(0x10, 6)], Opcode::ExpI(0x12, 0x10, 3), 0x12, 216);
}

#[test]
fn gt() {
    alu(&[(0x10, 6), (0x11, 3)], Opcode::GT(0x12, 0x10, 0x11), 0x12, 1);
    alu(&[(0x10, 3), (0x11, 3)], Opcode::GT(0x12, 0x10, 0x11), 0x12, 0);
    alu(&[(0x10, 1), (0x11, 3)], Opcode::GT(0x12, 0x10, 0x11), 0x12, 0);
}
