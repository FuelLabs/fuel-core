use super::common;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

#[test]
fn memcopy() {
    let mut vm = Interpreter::default();
    let tx = common::dummy_tx();
    vm.init(&tx).expect("Failed to init VM");

    let alloc = 1024;

    // r[0x10] := 1024
    vm.execute(Opcode::AddI(0x10, 0x10, alloc)).unwrap();
    vm.execute(Opcode::Aloc(0x10)).unwrap();

    // r[0x20] := 128
    vm.execute(Opcode::AddI(0x20, 0x20, 128)).unwrap();

    // r[0x11] := $hp + 1
    vm.execute(Opcode::Add(0x11, REG_ONE, REG_HP)).unwrap();

    for i in 0..alloc {
        vm.execute(Opcode::AddI(0x21, REG_ZERO, i)).unwrap();
        vm.execute(Opcode::SB(0x11, 0x21, i as Immediate12)).unwrap();
    }

    // r[0x23] := m[0x11, 0x20] == m[0x12, 0x20]
    vm.execute(Opcode::MemEq(0x23, 0x11, 0x12, 0x20)).unwrap();
    assert_eq!(0, vm.registers()[0x23]);

    // r[0x12] := 0x11 + r[0x20]
    vm.execute(Opcode::Add(0x12, 0x11, 0x20)).unwrap();

    // Test ownership
    vm.execute(Opcode::MCp(0x11, 0x12, 0x20)).unwrap();

    // r[0x23] := m[0x11, 0x20] == m[0x12, 0x20]
    vm.execute(Opcode::MemEq(0x23, 0x11, 0x12, 0x20)).unwrap();
    assert_eq!(1, vm.registers()[0x23]);

    // Assert ownership
    vm.execute(Opcode::SubI(0x24, 0x11, 1)).unwrap();
    let ownership_violated = vm.execute(Opcode::MCp(0x24, 0x12, 0x20));
    assert!(ownership_violated.is_err());

    // Assert no panic on overlapping
    vm.execute(Opcode::SubI(0x25, 0x12, 1)).unwrap();
    let overlapping = vm.execute(Opcode::MCp(0x11, 0x25, 0x20));
    assert!(overlapping.is_err());
}
