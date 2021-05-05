use super::Interpreter;
use crate::consts::VM_MAX_RAM;
use crate::types::Word;

impl Interpreter {
    pub fn program_counter_inc(&mut self) -> bool {
        self.registers[0x03] = self.registers[0x03].saturating_add(4);
        true
    }

    pub fn jump(&mut self, j: Word) -> bool {
        let j = self.registers[0x0c].saturating_add(j.saturating_mul(4));

        if j > VM_MAX_RAM - 1 {
            false
        } else {
            self.registers[0x03] = j;

            true
        }
    }
}
