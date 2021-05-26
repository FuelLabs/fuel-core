use super::{Interpreter, MemoryRange};
use crate::consts::{MEM_MAX_ACCESS_SIZE, VM_MAX_RAM};
use crate::crypto;

use fuel_asm::Word;
use fuel_tx::crypto as tx_crypto;

impl Interpreter {
    pub fn ecrecover(&mut self, a: Word, b: Word, c: Word) -> bool {
        let (ax, overflow) = a.overflowing_add(64);
        let (bx, of) = b.overflowing_add(64);
        let overflow = overflow || of;
        let (cx, of) = c.overflowing_add(32);
        let overflow = overflow || of;

        let range = MemoryRange::new(a, 64);
        if overflow || ax > VM_MAX_RAM || bx > VM_MAX_RAM || cx > VM_MAX_RAM || !self.has_ownership_range(&range) {
            false
        } else {
            let e = &self.memory[c as usize..cx as usize];
            let sig = &self.memory[b as usize..bx as usize];

            crypto::secp256k1_sign_compact_recover(&sig, &e)
                .map(|pk| {
                    self.memory[a as usize..ax as usize].copy_from_slice(&pk);
                    self.clear_err();
                })
                .unwrap_or_else(|_| {
                    self.memory[a as usize..ax as usize].copy_from_slice(&[0; 64]);
                    self.set_err();
                });

            true
        }
    }

    pub fn keccak256(&mut self, a: Word, b: Word, c: Word) -> bool {
        use sha3::{Digest, Keccak256};

        let (ax, overflow) = a.overflowing_add(32);
        let (bc, of) = b.overflowing_add(c);
        let overflow = overflow || of;

        let range = MemoryRange::new(a, 32);
        if overflow
            || ax > VM_MAX_RAM
            || bc > VM_MAX_RAM
            || c > MEM_MAX_ACCESS_SIZE
            || !self.has_ownership_range(&range)
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
        let (ax, overflow) = a.overflowing_add(32);
        let (bc, of) = b.overflowing_add(c);
        let overflow = overflow || of;

        let range = MemoryRange::new(a, 32);
        if overflow
            || ax > VM_MAX_RAM
            || bc > VM_MAX_RAM
            || c > MEM_MAX_ACCESS_SIZE
            || !self.has_ownership_range(&range)
        {
            false
        } else {
            let result = tx_crypto::hash(&self.memory[b as usize..bc as usize]);

            self.memory[a as usize..ax as usize].copy_from_slice(&result);
            true
        }
    }
}
