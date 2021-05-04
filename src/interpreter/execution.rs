use super::{Context, Interpreter};
use crate::consts::{RegisterId, Word};
use crate::opcodes::Opcode;

use std::ops::Div;

impl Interpreter {
    pub fn execute(&mut self, ctx: &mut Context, op: Opcode) {
        self.gas_used += op.gas_cost();
        if self.gas_used > self.gas_limit {
            // TODO implement out of gas exception
        }

        match op {
            Opcode::Add(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_add,
                    self.registers[rb],
                    self.registers[rc],
                );
            }

            Opcode::AddI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_add, self.registers[rb], imm as Word);
            }

            Opcode::And(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] & self.registers[rc])
            }

            Opcode::AndI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] & (imm as Word))
            }

            Opcode::Div(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_error(
                    ra,
                    Word::div,
                    self.registers[rb],
                    self.registers[rc],
                    self.registers[rc] == 0,
                );
            }

            Opcode::DivI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(ra, Word::div, self.registers[rb], imm as Word, imm == 0);
            }

            Opcode::Eq(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] == self.registers[rc]) as Word);
            }

            Opcode::Exp(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_pow,
                    self.registers[rb],
                    self.registers[rc] as u32,
                );
            }

            Opcode::ExpI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_pow, self.registers[rb], imm as u32);
            }

            Opcode::GT(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] == self.registers[rc]) as Word);
            }

            Opcode::MathLog(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_error(
                    ra,
                    |b, c| (b as f64).log(c as f64).trunc() as Word,
                    self.registers[rb],
                    self.registers[rc],
                    self.registers[rb] == 0 || self.registers[rc] <= 1,
                );
            }

            Opcode::MathRoot(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_error(
                    ra,
                    |b, c| (b as f64).powf((c as f64).recip()).trunc() as Word,
                    self.registers[rb],
                    self.registers[rc],
                    self.registers[rc] == 0,
                );
            }

            Opcode::Mod(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_error(
                    ra,
                    Word::wrapping_rem,
                    self.registers[rb],
                    self.registers[rc],
                    self.registers[rc] == 0,
                );
            }

            Opcode::ModI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(
                    ra,
                    Word::wrapping_rem,
                    self.registers[rb],
                    imm as Word,
                    imm == 0,
                );
            }

            Opcode::Mul(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_mul,
                    self.registers[rb],
                    self.registers[rc],
                );
            }

            Opcode::MulI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_mul, self.registers[rb], imm as Word);
            }

            Opcode::Noop => self.alu_clear(),

            Opcode::Not(ra, rb) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, !self.registers[rb]);
            }

            Opcode::Or(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] | self.registers[rc]);
            }

            Opcode::OrI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] | (imm as Word));
            }

            Opcode::SLL(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_shl,
                    self.registers[rb],
                    self.registers[rc] as u32,
                );
            }

            Opcode::SLLI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_shl, self.registers[rb], imm as u32);
            }

            Opcode::SRL(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_shr,
                    self.registers[rb],
                    self.registers[rc] as u32,
                );
            }

            Opcode::SRLI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_shr, self.registers[rb], imm as u32);
            }

            Opcode::Sub(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(
                    ra,
                    Word::overflowing_sub,
                    self.registers[rb],
                    self.registers[rc],
                );
            }

            Opcode::SubI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_sub, self.registers[rb], imm as Word);
            }

            Opcode::Xor(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] ^ self.registers[rc]);
            }

            Opcode::XorI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] ^ (imm as Word));
            }

            // TODO CIMV: Check input maturity verify
            // TODO CTMV: Check transaction maturity verify
            Opcode::JI(imm) if self.jump(imm as Word) => {}

            Opcode::JNEI(ra, rb, imm)
                if Self::is_valid_register_couple(ra, rb)
                    && (self.registers[ra] != self.registers[rb] && self.jump(imm as Word)
                        || self.program_counter_inc()) => {}

            // TODO RETURN: Return from context
            Opcode::CFEI(imm)
                if self.stack_pointer_overflow(Word::overflowing_add, imm as Word) => {}

            Opcode::CFSI(imm)
                if self.stack_pointer_overflow(Word::overflowing_sub, imm as Word) => {}

            Opcode::LB(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb)
                    && self.load_byte(ctx, ra, rb, imm as RegisterId) => {}

            Opcode::LW(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb)
                    && self.load_word(ctx, ra, rb, imm as RegisterId) => {}
            _ => {
                // TODO implement recoverable panic with meaningful messages
                panic!("Invalid instruction: {:?}", op);
            }
        }
    }
}
