use super::Interpreter;
use crate::consts::{MEM_MAX_ACCESS_SIZE, VM_MAX_RAM};
use crate::types::Word;

use std::convert::TryFrom;

impl Interpreter {
    pub fn ecrecover(&mut self, a: Word, b: Word, c: Word) -> bool {
        use secp256k1::recovery::{RecoverableSignature, RecoveryId};
        use secp256k1::{Message, Secp256k1};

        let (ax, overflow) = a.overflowing_add(64);
        let (bx, of) = b.overflowing_add(64);
        let overflow = overflow || of;
        let (cx, of) = c.overflowing_add(32);
        let overflow = overflow || of;
        self.inc_pc();

        if overflow
            || ax > VM_MAX_RAM
            || bx > VM_MAX_RAM
            || cx > VM_MAX_RAM
            || !self.has_ownership_range(a, ax)
        {
            false
        } else {
            let e = &self.memory[c as usize..cx as usize];

            // TODO test the scheme
            // recovery id is derived from a compression strategy described in
            // https://github.com/lazyledger/lazyledger-specs/blob/master/specs/data_structures.md#public-key-cryptography
            let mut sig = <[u8; 64]>::try_from(&self.memory[b as usize..bx as usize])
                .unwrap_or_else(|_| unreachable!());

            let v = 27 + (sig[32] >> 7);
            sig[32] &= 0x7f;

            Message::from_slice(e)
                .and_then(|message| {
                    let v = RecoveryId::from_i32(v as i32)?;
                    let sig = RecoverableSignature::from_compact(&sig, v)?;

                    Secp256k1::new().recover(&message, &sig)
                })
                .map(|secp| {
                    // Ignore the first byte of the compressed flag
                    let secp = secp.serialize_uncompressed();
                    self.memory[a as usize..ax as usize].copy_from_slice(&secp[1..]);

                    true
                })
                .unwrap_or(false)
        }
    }

    pub fn keccak256(&mut self, a: Word, b: Word, c: Word) -> bool {
        use sha3::{Digest, Keccak256};

        let (ax, overflow) = a.overflowing_add(32);
        let (bc, of) = b.overflowing_add(c);
        let overflow = overflow || of;
        self.inc_pc();

        if overflow
            || ax > VM_MAX_RAM
            || bc > VM_MAX_RAM
            || c > MEM_MAX_ACCESS_SIZE
            || !self.has_ownership_range(a, ax)
        {
            false
        } else {
            let mut h = Keccak256::new();
            h.update(&self.memory[b as usize..bc as usize]);
            let result = h.finalize();

            self.memory[a as usize..ax as usize].copy_from_slice(&result);
            true
        }
    }

    pub fn sha256(&mut self, a: Word, b: Word, c: Word) -> bool {
        use sha2::{Digest, Sha256};

        let (ax, overflow) = a.overflowing_add(32);
        let (bc, of) = b.overflowing_add(c);
        let overflow = overflow || of;
        self.inc_pc();

        if overflow
            || ax > VM_MAX_RAM
            || bc > VM_MAX_RAM
            || c > MEM_MAX_ACCESS_SIZE
            || !self.has_ownership_range(a, ax)
        {
            false
        } else {
            let mut h = Sha256::new();
            h.update(&self.memory[b as usize..bc as usize]);
            let result = h.finalize();

            self.memory[a as usize..ax as usize].copy_from_slice(&result);
            true
        }
    }
}

#[test]
fn sha256() {
    use crate::prelude::*;
    use sha2::{Digest, Sha256};

    let message = b"I say let the world go to hell, but I should always have my tea.";
    let mut hasher = Sha256::new();
    hasher.update(message);
    let hash = hasher.finalize();

    let vm = Interpreter::default();
    let mut vm = vm.init();

    // r[0x10] := 128
    vm.execute(Opcode::AddI(0x10, 0x10, 128));
    vm.execute(Opcode::Malloc(0x10));

    // r[0x12] := r[hp]
    vm.execute(Opcode::Move(0x12, 0x07));

    // m[hp + 1..hp + |message| + |hash| + 1] <- message + hash
    message
        .iter()
        .chain(hash.iter())
        .enumerate()
        .for_each(|(i, b)| {
            vm.execute(Opcode::AddI(0x11, 0x13, (*b) as Immediate12));
            vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12));
        });

    // Set message address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::AddI(0x11, 0x12, 1));

    // Set hash address to 0x14
    // r[0x14] := r[0x11] + |message|
    vm.execute(Opcode::AddI(0x14, 0x11, message.len() as Immediate12));

    // Set calculated hash address to 0x15
    // r[0x15] := r[0x14] + |hash|
    vm.execute(Opcode::AddI(0x15, 0x14, hash.len() as Immediate12));

    // Set hash size
    // r[0x16] := |message|
    vm.execute(Opcode::AddI(0x16, 0x16, message.len() as Immediate12));

    // Compute the SHA256
    vm.execute(Opcode::Sha256(0x15, 0x11, 0x16));

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    vm.execute(Opcode::AddI(0x18, 0x18, hash.len() as Immediate12));
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18));
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the message
    // r[0x19] := 0xff
    // m[message] := r[0x19]
    vm.execute(Opcode::AddI(0x19, 0x19, 0xff));
    vm.execute(Opcode::SB(0x11, 0x19, 0));

    // Compute the SHA256 of the corrupted message
    vm.execute(Opcode::Sha256(0x15, 0x11, 0x16));

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    // r[0x17] must be false
    vm.execute(Opcode::MemEq(0x17, 0x14, 0x15, 0x18));
    assert_eq!(0x00, vm.registers()[0x17]);
}
