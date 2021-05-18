use super::{ExecuteError, Interpreter};

use fuel_asm::{Opcode, RegisterId, Word};
use tracing::debug;

use std::ops::Div;

impl Interpreter {
    pub fn execute(&mut self, op: Opcode) -> Result<(), ExecuteError> {
        let mut result = Ok(());

        debug!("Executing {:?}", op);
        debug!(
            "Current state: {:?}",
            op.registers()
                .iter()
                .filter_map(|r| *r)
                .map(|r| (r, self.registers.get(r).copied()))
                .collect::<Vec<(RegisterId, Option<Word>)>>()
        );

        match op {
            Opcode::ADD(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_add, self.registers[rb], self.registers[rc])
            }

            Opcode::ADDI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_add, self.registers[rb], imm as Word)
            }

            Opcode::AND(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] & self.registers[rc])
            }

            Opcode::ANDI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] & (imm as Word))
            }

            Opcode::DIV(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                Word::div,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::DIVI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(ra, Word::div, self.registers[rb], imm as Word, imm == 0)
            }

            Opcode::EQ(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] == self.registers[rc]) as Word)
            }

            Opcode::EXP(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_pow, self.registers[rb], self.registers[rc] as u32)
            }

            Opcode::EXPI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_pow, self.registers[rb], imm as u32)
            }

            Opcode::GT(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, (self.registers[rb] > self.registers[rc]) as Word)
            }

            Opcode::MLOG(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                |b, c| (b as f64).log(c as f64).trunc() as Word,
                self.registers[rb],
                self.registers[rc],
                self.registers[rb] == 0 || self.registers[rc] <= 1,
            ),

            Opcode::MROO(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                |b, c| (b as f64).powf((c as f64).recip()).trunc() as Word,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::MOD(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => self.alu_error(
                ra,
                Word::wrapping_rem,
                self.registers[rb],
                self.registers[rc],
                self.registers[rc] == 0,
            ),

            Opcode::MODI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_error(ra, Word::wrapping_rem, self.registers[rb], imm as Word, imm == 0)
            }

            Opcode::MOVE(ra, rb) if Self::is_valid_register_couple_alu(ra, rb) => self.alu_set(ra, self.registers[rb]),

            Opcode::MUL(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_mul, self.registers[rb], self.registers[rc])
            }

            Opcode::MULI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_mul, self.registers[rb], imm as Word)
            }

            Opcode::NOOP => self.alu_clear(),

            Opcode::NOT(ra, rb) if Self::is_valid_register_couple_alu(ra, rb) => self.alu_set(ra, !self.registers[rb]),

            Opcode::OR(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] | self.registers[rc])
            }

            Opcode::ORI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
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

            Opcode::SUB(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_overflow(ra, Word::overflowing_sub, self.registers[rb], self.registers[rc])
            }

            Opcode::SUBI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_overflow(ra, Word::overflowing_sub, self.registers[rb], imm as Word)
            }

            Opcode::XOR(ra, rb, rc) if Self::is_valid_register_triple_alu(ra, rb, rc) => {
                self.alu_set(ra, self.registers[rb] ^ self.registers[rc])
            }

            Opcode::XORI(ra, rb, imm) if Self::is_valid_register_couple_alu(ra, rb) => {
                self.alu_set(ra, self.registers[rb] ^ (imm as Word))
            }

            // TODO CIMV: Check input maturity verify
            // TODO CTMV: Check transaction maturity verify
            Opcode::JI(imm) if self.jump(imm as Word) => {}

            Opcode::JNEI(ra, rb, imm)
                if Self::is_valid_register_couple(ra, rb)
                    && (self.registers[ra] != self.registers[rb] && self.jump(imm as Word) || self.inc_pc()) => {}

            // TODO RET: Return from context
            Opcode::CFEI(imm) if self.stack_pointer_overflow(Word::overflowing_add, imm as Word) => {}

            Opcode::CFSI(imm) if self.stack_pointer_overflow(Word::overflowing_sub, imm as Word) => {}

            Opcode::LB(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb) && self.load_byte(ra, rb, imm as RegisterId) => {}

            Opcode::LW(ra, rb, imm)
                if Self::is_valid_register_couple_alu(ra, rb) && self.load_word(ra, rb, imm as RegisterId) => {}

            Opcode::ALOC(ra) if Self::is_valid_register(ra) && self.malloc(self.registers[ra]) => {}

            Opcode::MCL(ra, rb)
                if Self::is_valid_register_couple(ra, rb) && self.memclear(self.registers[ra], self.registers[rb]) => {}

            Opcode::MCLI(ra, imm) if Self::is_valid_register(ra) && self.memclear(self.registers[ra], imm as Word) => {}

            Opcode::MCP(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.memcopy(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::MEQ(ra, rb, rc, rd)
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
            Opcode::CALL(ra, rb, rc, rd)
                if Self::is_valid_register_quadruple(ra, rb, rc, rd)
                    && self.call(
                        self.registers[ra],
                        self.registers[rb],
                        self.registers[rc],
                        self.registers[rd],
                    ) => {}

            // TODO CODECOPY: Code copy
            // TODO CODEROOT: Code Merkle root
            // TODO CODESIZE: Code size
            // TODO COINBASE: Block proposer address
            // TODO LOADCODE: Load code from an external contract
            Opcode::LOG(ra, rb, rc, rd)
                if Self::is_valid_register_quadruple(ra, rb, rc, rd) && self.log_append(&[ra, rb, rc, rd]) => {}

            // TODO MINT: Mint new coins
            // TODO REVERT: Revert
            // TODO SLOADCODE: Load code from static list
            // TODO SRW: State read word
            // TODO SRWQ: State read 32 bytes
            // TODO SWW: State write word
            // TODO SWWQ: State write 32 bytes
            // TODO TRANSFER: Transfer coins to contract
            // TODO TRANSFEROUT: Transfer coins to output
            Opcode::ECR(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.ecrecover(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::K256(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.keccak256(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::S256(ra, rb, rc)
                if Self::is_valid_register_triple(ra, rb, rc)
                    && self.sha256(self.registers[ra], self.registers[rb], self.registers[rc]) => {}

            Opcode::FLAG(ra) if Self::is_valid_register(ra) => self.set_flag(self.registers[ra]),

            _ => result = Err(ExecuteError::OpcodeFailure(op)),
        }

        debug!(
            "After   state: {:?}",
            op.registers()
                .iter()
                .filter_map(|r| *r)
                .map(|r| (r, self.registers.get(r).copied()))
                .collect::<Vec<(RegisterId, Option<Word>)>>()
        );

        result
    }
}
