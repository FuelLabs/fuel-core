use super::Interpreter;
use crate::consts::*;

use fuel_asm::{RegisterId, Word};

use std::convert::TryFrom;
use std::ptr;

mod range;

pub use range::MemoryRange;

impl Interpreter {
    /// Grant ownership of the range `[a..ab[`
    pub const fn has_ownership_range(&self, range: &MemoryRange) -> bool {
        let (a, ab) = range.boundaries(self);

        let a_is_stack = a < self.registers[REG_SP];
        let a_is_heap = a > self.registers[REG_HP];

        let ab_is_stack = ab <= self.registers[REG_SP];
        let ab_is_heap = ab >= self.registers[REG_HP];

        a < ab
            && (a_is_stack && ab_is_stack && self.has_ownership_stack(a) && self.has_ownership_stack_exclusive(ab)
                || a_is_heap && ab_is_heap && self.has_ownership_heap(a) && self.has_ownership_heap(ab))
    }

    pub const fn has_ownership_stack(&self, a: Word) -> bool {
        a <= VM_MAX_RAM && self.registers[REG_SSP] <= a && a < self.registers[REG_SP]
    }

    pub const fn has_ownership_stack_exclusive(&self, a: Word) -> bool {
        a <= VM_MAX_RAM && self.registers[REG_SSP] <= a && a <= self.registers[REG_SP]
    }

    pub const fn has_ownership_heap(&self, a: Word) -> bool {
        // TODO implement fp->hp and (addr, size) validations
        // fp->hp
        // it means $hp from the previous context, i.e. what's saved in the
        // "Saved registers from previous context" of the call frame at
        // $fp`
        a <= VM_MAX_RAM && (a < VM_MAX_RAM - 1 || !self.is_external_context()) && self.registers[REG_HP] < a
    }

    pub const fn is_stack_address(&self, a: Word) -> bool {
        a < self.registers[REG_SP]
    }
}

impl Interpreter {
    pub fn stack_pointer_overflow(&mut self, f: fn(Word, Word) -> (Word, bool), v: Word) -> bool {
        let (result, overflow) = f(self.registers[REG_SP], v);

        if overflow || result > self.registers[REG_HP] {
            false
        } else {
            self.registers[REG_SP] = result;
            true
        }
    }

    pub fn load_byte(&mut self, ra: RegisterId, b: RegisterId, c: Word) -> bool {
        let bc = b.saturating_add(c as RegisterId);

        if bc >= VM_MAX_RAM as RegisterId {
            false
        } else {
            // TODO ensure the byte should be cast and not overwrite the
            // encoding
            self.registers[ra] = self.memory[bc] as Word;

            true
        }
    }

    pub fn load_word(&mut self, ra: RegisterId, b: Word, c: Word) -> bool {
        // C is expressed in words; mul by 8
        let (bc, overflow) = b.overflowing_add(c * 8);
        let (bcw, of) = bc.overflowing_add(8);
        let overflow = overflow || of;

        let bc = bc as usize;
        let bcw = bcw as usize;

        if overflow || bcw >= VM_MAX_RAM as RegisterId {
            false
        } else {
            // Safe conversion of sized slice
            self.registers[ra] = <[u8; 8]>::try_from(&self.memory[bc..bcw])
                .map(Word::from_be_bytes)
                .unwrap_or_else(|_| unreachable!());

            true
        }
    }

    pub fn store_byte(&mut self, a: Word, b: Word, c: Word) -> bool {
        let (ac, overflow) = a.overflowing_add(c);
        let (result, of) = ac.overflowing_add(1);
        let overflow = overflow || of;

        if overflow || result > VM_MAX_RAM || !(self.has_ownership_stack(ac) || self.has_ownership_heap(ac)) {
            false
        } else {
            self.memory[ac as usize] = b.to_le_bytes()[0];
            true
        }
    }

    pub fn store_word(&mut self, a: Word, b: Word, c: Word) -> bool {
        // C is expressed in words; mul by 8
        let (ac, overflow) = a.overflowing_add(c * 8);
        let (acw, of) = ac.overflowing_add(8);
        let overflow = overflow || of;

        let range = MemoryRange::new(ac, 8);
        if overflow || acw > VM_MAX_RAM || !self.has_ownership_range(&range) {
            false
        } else {
            // TODO review if BE is intended
            self.memory[ac as usize..acw as usize].copy_from_slice(&b.to_be_bytes());

            true
        }
    }

    pub fn malloc(&mut self, a: Word) -> bool {
        let (result, overflow) = self.registers[REG_HP].overflowing_sub(a);

        if overflow || result < self.registers[REG_SP] {
            false
        } else {
            self.registers[REG_HP] = result;
            true
        }
    }

    pub fn memclear(&mut self, a: Word, b: Word) -> bool {
        let (ab, overflow) = a.overflowing_add(b);

        let range = MemoryRange::new(a, b);
        if overflow || ab > VM_MAX_RAM || b > MEM_MAX_ACCESS_SIZE || !self.has_ownership_range(&range) {
            false
        } else {
            // trivial compiler optimization for memset when best
            for i in &mut self.memory[a as usize..ab as usize] {
                *i = 0
            }
            true
        }
    }

    pub fn memcopy(&mut self, a: Word, b: Word, c: Word) -> bool {
        let (ac, overflow) = a.overflowing_add(c);
        let (bc, of) = b.overflowing_add(c);
        let overflow = overflow || of;

        let range = MemoryRange::new(a, c);
        if overflow
            || ac > VM_MAX_RAM
            || bc > VM_MAX_RAM
            || c > MEM_MAX_ACCESS_SIZE
            || a <= b && b < ac
            || b <= a && a < bc
            || !self.has_ownership_range(&range)
        {
            false
        } else {
            // The pointers are granted to be aligned so this is a safe
            // operation
            let src = &self.memory[b as usize] as *const u8;
            let dst = &mut self.memory[a as usize] as *mut u8;

            unsafe {
                ptr::copy_nonoverlapping(src, dst, c as usize);
            }

            true
        }
    }

    pub fn memeq(&mut self, ra: RegisterId, b: Word, c: Word, d: Word) -> bool {
        let (bd, overflow) = b.overflowing_add(d);
        let (cd, of) = c.overflowing_add(d);
        let overflow = overflow || of;

        if overflow || bd > VM_MAX_RAM || cd > VM_MAX_RAM || d > MEM_MAX_ACCESS_SIZE {
            false
        } else {
            self.registers[ra] = (self.memory[b as usize..bd as usize] == self.memory[c as usize..cd as usize]) as Word;

            true
        }
    }
}
