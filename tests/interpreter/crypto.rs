use super::common;
use fuel_vm_rust::prelude::*;

#[test]
fn sha256() {
    use sha2::{Digest, Sha256};

    let message = b"I say let the world go to hell, but I should always have my tea.";
    let mut hasher = Sha256::new();
    hasher.update(message);
    let hash = hasher.finalize();

    let vm = Interpreter::default();
    let tx = common::dummy_tx();
    let mut vm = vm.init(&tx);

    // r[0x10] := 128
    vm.execute(Opcode::AddI(0x10, 0x10, 128)).unwrap();
    vm.execute(Opcode::Malloc(0x10)).unwrap();

    // r[0x12] := r[hp]
    vm.execute(Opcode::Move(0x12, 0x07)).unwrap();

    // m[hp + 1..hp + |message| + |hash| + 1] <- message + hash
    message.iter().chain(hash.iter()).enumerate().for_each(|(i, b)| {
        vm.execute(Opcode::AddI(0x11, 0x13, (*b) as Immediate12)).unwrap();
        vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12)).unwrap();
    });

    // Set message address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::AddI(0x11, 0x12, 1)).unwrap();

    // Set hash address to 0x14
    // r[0x14] := r[0x11] + |message|
    vm.execute(Opcode::AddI(0x14, 0x11, message.len() as Immediate12))
        .unwrap();

    // Set calculated hash address to 0x15
    // r[0x15] := r[0x14] + |hash|
    vm.execute(Opcode::AddI(0x15, 0x14, hash.len() as Immediate12)).unwrap();

    // Set hash size
    // r[0x16] := |message|
    vm.execute(Opcode::AddI(0x16, 0x16, message.len() as Immediate12))
        .unwrap();

    // Compute the SHA256
    vm.execute(Opcode::Sha256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    vm.execute(Opcode::AddI(0x18, 0x18, hash.len() as Immediate12)).unwrap();
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the message
    // r[0x19] := 0xff
    // m[message] := r[0x19]
    vm.execute(Opcode::AddI(0x19, 0x19, 0xff)).unwrap();
    vm.execute(Opcode::SB(0x11, 0x19, 0)).unwrap();

    // Compute the SHA256 of the corrupted message
    vm.execute(Opcode::Sha256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    // r[0x17] must be false
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x00, vm.registers()[0x17]);
}

#[test]
fn keccak256() {
    use sha3::{Digest, Keccak256};

    let message = b"...and, moreover, I consider it my duty to warn you that the cat is an ancient, inviolable animal.";
    let mut hasher = Keccak256::new();
    hasher.update(message);
    let hash = hasher.finalize();

    let vm = Interpreter::default();
    let tx = common::dummy_tx();
    let mut vm = vm.init(&tx);

    // r[0x10] := 162
    vm.execute(Opcode::AddI(0x10, 0x10, 162)).unwrap();
    vm.execute(Opcode::Malloc(0x10)).unwrap();

    // r[0x12] := r[hp]
    vm.execute(Opcode::Move(0x12, 0x07)).unwrap();

    // m[hp + 1..hp + |message| + |hash| + 1] <- message + hash
    message.iter().chain(hash.iter()).enumerate().for_each(|(i, b)| {
        vm.execute(Opcode::AddI(0x11, 0x13, (*b) as Immediate12)).unwrap();
        vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12)).unwrap();
    });

    // Set message address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::AddI(0x11, 0x12, 1)).unwrap();

    // Set hash address to 0x14
    // r[0x14] := r[0x11] + |message|
    vm.execute(Opcode::AddI(0x14, 0x11, message.len() as Immediate12))
        .unwrap();

    // Set calculated hash address to 0x15
    // r[0x15] := r[0x14] + |hash|
    vm.execute(Opcode::AddI(0x15, 0x14, hash.len() as Immediate12)).unwrap();

    // Set hash size
    // r[0x16] := |message|
    vm.execute(Opcode::AddI(0x16, 0x16, message.len() as Immediate12))
        .unwrap();

    // Compute the Keccak256
    vm.execute(Opcode::Keccak256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    vm.execute(Opcode::AddI(0x18, 0x18, hash.len() as Immediate12)).unwrap();
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the message
    // r[0x19] := 0xff
    // m[message] := r[0x19]
    vm.execute(Opcode::AddI(0x19, 0x19, 0xff)).unwrap();
    vm.execute(Opcode::SB(0x11, 0x19, 0)).unwrap();

    // Compute the Keccak256 of the corrupted message
    vm.execute(Opcode::Keccak256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    // r[0x17] must be false
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x00, vm.registers()[0x17]);
}
