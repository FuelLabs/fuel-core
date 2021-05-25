use super::{Call, Color, Interpreter};
use crate::consts::*;

use fuel_asm::Word;

use std::convert::TryFrom;
use std::io::{Read, Write};

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

    pub fn call(&mut self, a: Word, _b: Word, c: Word, _d: Word) -> bool {
        let (ax, overflow) = a.overflowing_add(32);
        let (cx, of) = c.overflowing_add(32);
        let overflow = overflow || of;

        let mut call = Call::default();

        // TODO validate external and internal context
        if overflow
            || ax > VM_MAX_RAM
            || cx > VM_MAX_RAM
            || call.write(&self.memory[a as usize..]).is_err()
            || !self.tx.input_contracts().any(|contract| call.to() == contract)
            || !call
                .outputs()
                .iter()
                .fold(true, |acc, output| acc && self.has_ownership_range(output))
        {
            false
        } else {
            let color = Color::try_from(&self.memory[c as usize..cx as usize]).unwrap_or_else(|_| unreachable!());
            // TODO update color balance

            let mut frame = self.call_frame(call, color);

            let sp = self.registers[REG_SP];
            self.registers[REG_FP] = self.registers[REG_SP];
            match frame.read(&mut self.memory[sp as usize..]) {
                Ok(n) => self.registers[REG_SP] = self.registers[REG_SP].saturating_add(n as Word),
                Err(_) => return false,
            }
            self.registers[REG_SSP] = self.registers[REG_SP];
            self.registers[REG_PC] = self.registers[REG_SP].saturating_sub(frame.code().len() as Word);
            self.registers[REG_IS] = self.registers[REG_PC];

            // TODO set balance for forward coins to $bal
            // TODO set forward gas to $cgas

            self.frames.push(frame);

            true
        }
    }
}
