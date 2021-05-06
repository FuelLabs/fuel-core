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

        if overflow || ax > VM_MAX_RAM || bx > VM_MAX_RAM || cx > VM_MAX_RAM || !self.has_ownership_range(a, ax) {
            false
        } else {
            let e = &self.memory[c as usize..cx as usize];

            // TODO test the scheme
            // recovery id is derived from a compression strategy described in
            // https://github.com/lazyledger/lazyledger-specs/blob/master/specs/data_structures.md#public-key-cryptography
            let mut sig =
                <[u8; 64]>::try_from(&self.memory[b as usize..bx as usize]).unwrap_or_else(|_| unreachable!());

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

        if overflow || ax > VM_MAX_RAM || bc > VM_MAX_RAM || c > MEM_MAX_ACCESS_SIZE || !self.has_ownership_range(a, ax)
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

        if overflow || ax > VM_MAX_RAM || bc > VM_MAX_RAM || c > MEM_MAX_ACCESS_SIZE || !self.has_ownership_range(a, ax)
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
