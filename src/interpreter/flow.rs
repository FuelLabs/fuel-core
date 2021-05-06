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
}
