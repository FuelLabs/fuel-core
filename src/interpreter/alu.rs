use super::Interpreter;
use crate::consts::*;

use fuel_asm::{RegisterId, Word};

impl Interpreter {
    pub fn alu_overflow<B, C>(&mut self, ra: RegisterId, f: fn(B, C) -> (Word, bool), b: B, c: C) {
        let (result, overflow) = f(b, c);

        // TODO If the F_UNSAFEMATH flag is unset, an operation that would have set $err
        // to true is instead a panic.
        //
        // TODO If the F_WRAPPING flag is unset, an operation that would have set $of to
        // a non-zero value is instead a panic.

        self.registers[REG_OF] = overflow as Word;
        self.registers[REG_ERR] = 0;
        self.inc_pc();

        self.registers[ra] = result;
    }

    pub fn alu_error<B, C>(&mut self, ra: RegisterId, f: fn(B, C) -> Word, b: B, c: C, err: bool) {
        self.registers[REG_OF] = 0;
        self.registers[REG_ERR] = err as Word;
        self.inc_pc();

        self.registers[ra] = if err { 0 } else { f(b, c) };
    }

    pub fn alu_set(&mut self, ra: RegisterId, b: Word) {
        self.registers[REG_OF] = 0;
        self.registers[REG_ERR] = 0;
        self.inc_pc();

        self.registers[ra] = b;
    }

    pub fn alu_clear(&mut self) {
        self.registers[REG_OF] = 0;
        self.registers[REG_ERR] = 0;
        self.inc_pc();
    }
}
