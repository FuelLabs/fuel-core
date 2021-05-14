use super::Interpreter;
use crate::consts::*;
use crate::types::Word;

impl Interpreter {
    pub fn jump(&mut self, j: Word) -> bool {
        let j = self.registers[REG_IS].saturating_add(j.saturating_mul(4));

        if j > VM_MAX_RAM - 1 {
            false
        } else {
            self.registers[REG_PC] = j;

            true
        }
    }

    /*
    pub fn call(&mut self, a: Word, b: Word, c: Word, d: Word) -> bool {
        let (ax, overflow) = a.overflowing_add(32);
        let (cx, of) = c.overflowing_add(32);
        let overflow = overflow || of;

        //self.tx.input_contracts().any(|contract| );

        true
    }
    */
}
