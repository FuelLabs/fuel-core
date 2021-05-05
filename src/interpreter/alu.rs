use super::Interpreter;
use crate::types::{RegisterId, Word};

impl Interpreter {
    pub fn alu_overflow<B, C>(&mut self, ra: RegisterId, f: fn(B, C) -> (Word, bool), b: B, c: C) {
        let (result, overflow) = f(b, c);

        self.registers[0x02] = overflow as Word;
        self.registers[0x08] = 0;
        self.inc_pc();

        self.registers[ra] = result;
    }

    pub fn alu_error<B, C>(&mut self, ra: RegisterId, f: fn(B, C) -> Word, b: B, c: C, err: bool) {
        self.registers[0x02] = 0;
        self.registers[0x08] = err as Word;
        self.inc_pc();

        self.registers[ra] = if err { 0 } else { f(b, c) };
    }

    pub fn alu_set(&mut self, ra: RegisterId, b: Word) {
        self.registers[0x02] = 0;
        self.registers[0x08] = 0;
        self.inc_pc();

        self.registers[ra] = b;
    }

    pub fn alu_clear(&mut self) {
        self.registers[0x02] = 0;
        self.registers[0x08] = 0;
        self.inc_pc();
    }
}
