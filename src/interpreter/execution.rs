use super::{ExecuteError, Interpreter};
use crate::opcodes::Opcode;
use crate::types::{RegisterId, Word};

use std::ops::Div;

impl Interpreter {
    pub fn execute(&mut self, op: Opcode) -> Result<(), ExecuteError> {
        let mut result = Ok(());

        match op {
            Opcode::Add(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_add, self.registers[rb], self.registers[rc])
            }

            Opcode::AddI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_add, self.registers[rb], imm as Word)
            }

            Opcode::And(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] & self.registers[rc])
            }

            Opcode::AndI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] & (imm as Word))
            }

            Opcode::Div(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                Word::div,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::DivI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(ra, Word::div, self.registers[rb], imm as Word, imm == 0)
            }

            Opcode::Eq(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] == self.registers[rc]) as Word)
            }

            Opcode::Exp(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_pow, self.registers[rb], self.registers[rc] as u32)
            }

            Opcode::ExpI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_pow, self.registers[rb], imm as u32)
            }

            Opcode::GT(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] > self.registers[rc]) as Word)
            }

            Opcode::MathLog(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                |b, c| (b as f64).log(c as f64).trunc() as Word,
                self.registers[rb],
                self.registers[rc],
                self.registers[rb] == 0 || self.registers[rc] <= 1,
            ),

            Opcode::MathRoot(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                |b, c| (b as f64).powf((c as f64).recip()).trunc() as Word,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::Mod(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                Word::wrapping_rem,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::ModI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(ra, Word::wrapping_rem, self.registers[rb], imm as Word, imm == 0)
            }

            Opcode::Move(ra, rb) if Self::is_valid_register_couple_alu(ra, rb) => self.alu_set(ra, self.registers[rb]),

            Opcode::Mul(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_mul, self.registers[rb], self.registers[rc])
            }

            Opcode::MulI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_mul, self.registers[rb], imm as Word)
            }

            Opcode::Noop => self.alu_clear(),

            Opcode::Not(ra, rb) if Self::is_valid_register_couple_alu(ra, rb) => self.alu_set(ra, !self.registers[rb]),

            Opcode::Or(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] | self.registers[rc])
            }

            Opcode::OrI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] | (imm as Word))
            }

            Opcode::SLL(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_shl, self.registers[rb], self.registers[rc] as u32)
            }

            Opcode::SLLI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_shl, self.registers[rb], imm as u32)
            }

            Opcode::SRL(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_shr, self.registers[rb], self.registers[rc] as u32)
            }

            Opcode::SRLI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_shr, self.registers[rb], imm as u32)
            }

            Opcode::Sub(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_sub, self.registers[rb], self.registers[rc])
            }

            Opcode::SubI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_sub, self.registers[rb], imm as Word)
            }

            Opcode::Xor(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] ^ self.registers[rc])
            }

            Opcode::XorI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] ^ (imm as Word))
            }

            // TODO CIMV: Check input maturity verify
            // TODO CTMV: Check transaction maturity verify
            Opcode::JI(imm) if self.jump(imm as Word) => {}

            Opcode::JNEI(ra, rb, imm)
                if Self::is_valid_register_couple(ra, rb)
                    && (self.registers[ra] != self.registers[rb] && self.jump(imm as Word) || self.inc_pc()) => {}

            // TODO RETURN: Return from context
            Opcode::CFEI(imm) if self.stack_pointer_overflow(Word::overflowing_add, imm as Word) => {}

            Opcode::CFSI(imm) if self.stack_pointer_overflow(Word::overflowing_sub, imm as Word) => {}

            Opcode::LB(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb) && self.load_byte(ra, rb, imm as RegisterId) => {}

            Opcode::LW(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb) && self.load_word(ra, rb, imm as RegisterId) => {}

            Opcode::Malloc(ra) if Self::is_valid_register(ra) && self.malloc(self.registers[ra]) => {}

            Opcode::MemClear(ra, rb)
                if Self::is_valid_register_couple(ra, rb) && self.memclear(self.registers[ra], self.registers[rb]) => {}

            Opcode::MemCp(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.memcopy(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::MemEq(ra, rb, rc, rd)
                if Self::is_valid_register_quadruple_alu(ra, rb, rc, rd)
                    && self.memeq(ra, self.registers[rb], self.registers[rc], self.registers[rd]) => {}

            Opcode::SB(ra, rb, imm)
                if Self::is_valid_register_couple(ra, rb)
                    && self.store_byte(self.registers[ra], self.registers[rb], imm as Word) => {}

            Opcode::SW(ra, rb, imm)
                if Self::is_valid_register_couple(ra, rb)
                    && self.store_word(self.registers[ra], self.registers[rb], imm as Word) => {}

            // TODO BLOCKHASH: Block hash
            // TODO BLOCKHEIGHT: Block height
            // TODO BURN: Burn existing coins
            // TODO CALL: Call contract
            // TODO CODECOPY: Code copy
            // TODO CODEROOT: Code Merkle root
            // TODO CODESIZE: Code size
            // TODO COINBASE: Block proposer address
            // TODO LOADCODE: Load code from an external contract
            // TODO LOG: Log event
            // TODO MINT: Mint new coins
            // TODO REVERT: Revert
            // TODO SLOADCODE: Load code from static list
            // TODO SRW: State read word
            // TODO SRWX: State read 32 bytes
            // TODO SWW: State write word
            // TODO SWWX: State write 32 bytes
            // TODO TRANSFER: Transfer coins to contract
            // TODO TRANSFEROUT: Transfer coins to output
            Opcode::ECRecover(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.ecrecover(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::Keccak256(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.keccak256(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::Sha256(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.sha256(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::Flag(ra) if Self::is_valid_register(ra) => self.set_flag(self.registers[ra]),

            _ => result = Err(ExecuteError::OpcodeFailure(op)),
        }

        result
    }
}
