use fuel_core::consts::*;
use fuel_core::prelude::*;

#[test]
fn memcopy() {
    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    let alloc = 1024;

    // r[0x10] := 1024
    vm.execute(Opcode::ADDI(0x10, 0x10, alloc)).unwrap();
    vm.execute(Opcode::ALOC(0x10)).unwrap();

    // r[0x20] := 128
    vm.execute(Opcode::ADDI(0x20, 0x20, 128)).unwrap();

    // r[0x11] := $hp + 1
    vm.execute(Opcode::ADD(0x11, REG_ONE, REG_HP)).unwrap();

    for i in 0..alloc {
        vm.execute(Opcode::ADDI(0x21, REG_ZERO, i)).unwrap();
        vm.execute(Opcode::SB(0x11, 0x21, i as Immediate12)).unwrap();
    }

    // r[0x23] := m[0x11, 0x20] == m[0x12, 0x20]
    vm.execute(Opcode::MEQ(0x23, 0x11, 0x12, 0x20)).unwrap();
    assert_eq!(0, vm.registers()[0x23]);

    // r[0x12] := 0x11 + r[0x20]
    vm.execute(Opcode::ADD(0x12, 0x11, 0x20)).unwrap();

    // Test ownership
    vm.execute(Opcode::MCP(0x11, 0x12, 0x20)).unwrap();

    // r[0x23] := m[0x11, 0x20] == m[0x12, 0x20]
    vm.execute(Opcode::MEQ(0x23, 0x11, 0x12, 0x20)).unwrap();
    assert_eq!(1, vm.registers()[0x23]);

    // Assert ownership
    vm.execute(Opcode::SUBI(0x24, 0x11, 1)).unwrap();
    let ownership_violated = vm.execute(Opcode::MCP(0x24, 0x12, 0x20));
    assert!(ownership_violated.is_err());

    // Assert no panic on overlapping
    vm.execute(Opcode::SUBI(0x25, 0x12, 1)).unwrap();
    let overlapping = vm.execute(Opcode::MCP(0x11, 0x25, 0x20));
    assert!(overlapping.is_err());
}

#[test]
fn memrange() {
    let m = MemoryRange::from(..1024);
    let m_p = MemoryRange::new(0, 1024);
    assert_eq!(m, m_p);

    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    let bytes = 1024;
    vm.execute(Opcode::ADDI(0x10, REG_ZERO, bytes as Immediate12)).unwrap();
    vm.execute(Opcode::ALOC(0x10)).unwrap();

    let m = MemoryRange::new(vm.registers()[REG_HP], bytes);
    assert!(!vm.has_ownership_range(&m));

    let m = MemoryRange::new(vm.registers()[REG_HP] + 1, bytes);
    assert!(vm.has_ownership_range(&m));

    let m = MemoryRange::new(vm.registers()[REG_HP] + 1, bytes + 1);
    assert!(!vm.has_ownership_range(&m));

    let m = MemoryRange::new(0, bytes).to_heap(&vm);
    assert!(vm.has_ownership_range(&m));

    let m = MemoryRange::new(0, bytes + 1).to_heap(&vm);
    assert!(!vm.has_ownership_range(&m));
}

#[test]
fn stack_alloc_ownership() {
    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    vm.execute(Opcode::MOVE(0x10, REG_SP)).unwrap();
    vm.execute(Opcode::CFEI(2)).unwrap();

    // Assert allocated stack is writable
    vm.execute(Opcode::MCLI(0x10, 2)).unwrap();
}
