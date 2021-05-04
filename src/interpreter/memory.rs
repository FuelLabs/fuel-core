use super::{Context, Interpreter};
use crate::consts::{RegisterId, Word, VM_MAX_RAM};

use std::convert::TryFrom;

impl Interpreter {
    pub fn stack_pointer_overflow(&mut self, f: fn(Word, Word) -> (Word, bool), v: Word) -> bool {
        let (result, overflow) = f(self.registers[0x05], v);

        if overflow || result > self.registers[0x07] {
            false
        } else {
            self.registers[0x05] = result;
            true
        }
    }

    pub fn load_byte(
        &mut self,
        ctx: &Context,
        ra: RegisterId,
        b: RegisterId,
        c: RegisterId,
    ) -> bool {
        let bc = b.saturating_add(c);

        if bc >= VM_MAX_RAM as RegisterId {
            false
        } else {
            // TODO ensure the byte should be cast and not overwrite the encoding
            self.registers[ra] = ctx.memory()[bc] as Word;

            true
        }
    }

    pub fn load_word(
        &mut self,
        ctx: &Context,
        ra: RegisterId,
        b: RegisterId,
        c: RegisterId,
    ) -> bool {
        let (bc, overflow) = b.overflowing_add(c);
        let (bcw, of) = bc.overflowing_add(8);
        let overflow = overflow || of;

        if overflow || bcw >= VM_MAX_RAM as RegisterId {
            false
        } else {
            // TODO check if it is expected to be BE
            // Safe conversion of sized slice
            self.registers[ra] = <[u8; 8]>::try_from(&ctx.memory()[bc..bc + 8])
                .map(|bytes| Word::from_be_bytes(bytes))
                .unwrap_or_else(|_| unreachable!());

            true
        }
    }
}
