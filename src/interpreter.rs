use crate::consts::*;

use fuel_asm::{Opcode, RegisterId, Word};
use fuel_tx::bytes::{SerializableVec, SizedBytes};
use fuel_tx::consts::*;
use fuel_tx::{Color, ContractAddress, Hash, Input, Output, Transaction};
use itertools::Itertools;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem;

mod alu;
mod blockchain;
mod contract;
mod crypto;
mod error;
mod execution;
mod executors;
mod flow;
mod frame;
mod log;
mod memory;

pub use contract::Contract;
pub use error::ExecuteError;
pub use frame::{Call, CallFrame};
pub use log::LogEvent;
pub use memory::MemoryRange;

const COLOR_SIZE: usize = mem::size_of::<Color>();
const HASH_SIZE: usize = mem::size_of::<Hash>();
const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Clone)]
pub struct Interpreter {
    registers: [Word; VM_REGISTER_COUNT],
    memory: Vec<u8>,
    frames: Vec<CallFrame>,
    log: Vec<LogEvent>,
    // TODO review all opcodes that mutates the tx in the stack and keep this one sync
    tx: Transaction,
    contracts: HashMap<ContractAddress, Contract>,
    color_balances: HashMap<Color, Word>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            registers: [0; VM_REGISTER_COUNT],
            memory: vec![0; VM_MAX_RAM as usize],
            frames: vec![],
            log: vec![],
            tx: Transaction::default(),
            contracts: HashMap::new(),
            color_balances: HashMap::new(),
        }
    }
}

impl Interpreter {
    pub fn init(&mut self, tx: Transaction) -> Result<(), ExecuteError> {
        self._init(tx, false)
    }

    fn _init(&mut self, mut tx: Transaction, predicate: bool) -> Result<(), ExecuteError> {
        tx.validate(self.block_height() as Word)?;

        self.frames.clear();
        self.log.clear();

        // Optimized for memset
        self.registers.iter_mut().for_each(|r| *r = 0);

        self.registers[REG_ONE] = 1;
        self.registers[REG_SSP] = 0;

        // Set heap area
        self.registers[REG_FP] = VM_MAX_RAM - 1;
        self.registers[REG_HP] = self.registers[REG_FP];

        self.push_stack(&tx.id())?;

        let zeroes = &[0; MAX_INPUTS as usize * (COLOR_SIZE + WORD_SIZE)];
        let mut ssp = self.registers[REG_SSP] as usize;

        self.push_stack(zeroes)?;
        if !predicate {
            tx.inputs()
                .iter()
                .filter_map(|input| match input {
                    Input::Coin { color, .. } => Some(color),
                    _ => None,
                })
                .sorted()
                .take(MAX_INPUTS as usize)
                .for_each(|color| {
                    let balance = self.color_balance(color);

                    self.memory[ssp..ssp + COLOR_SIZE].copy_from_slice(color);
                    ssp += COLOR_SIZE;

                    self.memory[ssp..ssp + WORD_SIZE].copy_from_slice(&balance.to_be_bytes());
                    ssp += WORD_SIZE;
                });
        }

        let tx_size = tx.serialized_size() as Word;
        self.push_stack(&tx_size.to_be_bytes())?;
        self.push_stack(tx.to_bytes().as_slice())?;

        self.registers[REG_SP] = self.registers[REG_SSP];

        self.tx = tx;

        Ok(())
    }

    pub fn run(&mut self) -> Result<(), ExecuteError> {
        let tx = &self.tx;

        match tx {
            Transaction::Create {
                salt, static_contracts, ..
            } => {
                if static_contracts.iter().any(|id| !self.check_contract_exists(id)) {
                    Err(ExecuteError::TransactionCreateStaticContractNotFound)?
                }

                let contract = Contract::try_from(tx)?;
                let id = contract.address(salt);
                if !tx.outputs().iter().any(|output| match output {
                    Output::ContractCreated { contract_id } if contract_id == &id => true,
                    _ => false,
                }) {
                    Err(ExecuteError::TransactionCreateIdNotInTx)?;
                }

                self.contracts.insert(id, contract);

                // Verify predicates
                // https://github.com/FuelLabs/fuel-specs/blob/master/specs/protocol/tx_validity.md#predicate-verification
                // TODO this should be abstracted with the client
                let predicates: Vec<MemoryRange> = tx
                    .inputs()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, input)| match input {
                        Input::Coin { predicate, .. } if !predicate.is_empty() => tx
                            .input_coin_predicate_offset(i)
                            .map(|ofs| (ofs as Word, predicate.len() as Word)),
                        _ => None,
                    })
                    .map(|(ofs, len)| (ofs + Self::tx_mem_address() as Word, len))
                    .map(|(ofs, len)| MemoryRange::new(ofs, len))
                    .collect();

                predicates
                    .iter()
                    .try_for_each(|predicate| self.verify_predicate(predicate))?;

                Ok(())
            }

            Transaction::Script { .. } => {
                let offset = (Self::tx_mem_address() + Transaction::script_offset()) as Word;

                self.registers[REG_PC] = offset;
                self.registers[REG_IS] = offset;
                self.registers[REG_GGAS] = self.tx.gas_limit();
                self.registers[REG_CGAS] = self.tx.gas_limit();

                // TODO set tree balance

                self.run_program()
            }
        }
    }

    pub fn run_program(&mut self) -> Result<(), ExecuteError> {
        loop {
            if self.registers[REG_PC] >= VM_MAX_RAM {
                return Err(ExecuteError::ProgramOverflow);
            }

            let op = self.memory[self.registers[REG_PC] as usize..]
                .chunks_exact(4)
                .next()
                .map(Opcode::from_bytes_unchecked)
                .ok_or(ExecuteError::ProgramOverflow)?;

            if let Opcode::RET(ra) = op {
                if Self::is_valid_register(ra) && self.ret(ra) && self.inc_pc() {
                    return Ok(());
                } else {
                    return Err(ExecuteError::OpcodeFailure(op));
                }
            }

            self.execute(op)?;
        }
    }

    pub fn verify_predicate(&mut self, predicate: &MemoryRange) -> Result<(), ExecuteError> {
        // TODO initialize VM with tx prepared for sign
        let (start, end) = predicate.boundaries(&self);

        self.registers[REG_PC] = start;
        self.registers[REG_IS] = start;

        // TODO optimize
        loop {
            let pc = self.registers[REG_PC];
            let op = self.memory[pc as usize..]
                .chunks_exact(Opcode::BYTES_SIZE)
                .next()
                .map(Opcode::from_bytes_unchecked)
                .ok_or(ExecuteError::PredicateOverflow)?;

            if let Opcode::RET(ra) = op {
                return self
                    .registers
                    .get(ra)
                    .ok_or(ExecuteError::OpcodeFailure(op))
                    .and_then(|ret| {
                        if ret == &1 {
                            Ok(())
                        } else {
                            Err(ExecuteError::PredicateFailure)
                        }
                    });
            }

            self.execute(op)?;

            if self.registers[REG_PC] < pc || self.registers[REG_PC] >= end {
                return Err(ExecuteError::PredicateOverflow);
            }
        }
    }

    pub fn push_stack(&mut self, data: &[u8]) -> Result<(), ExecuteError> {
        let (ssp, overflow) = self.registers[REG_SSP].overflowing_add(data.len() as Word);

        if overflow || ssp > self.registers[REG_FP] {
            Err(ExecuteError::StackOverflow)
        } else {
            self.memory[self.registers[REG_SSP] as usize..ssp as usize].copy_from_slice(data);
            self.registers[REG_SSP] = ssp;

            Ok(())
        }
    }

    pub fn push_stack_bypass_fp(&mut self, data: &[u8]) -> Result<(), ExecuteError> {
        let (ssp, overflow) = self.registers[REG_SSP].overflowing_add(data.len() as Word);

        if overflow {
            Err(ExecuteError::StackOverflow)
        } else {
            self.memory[self.registers[REG_SSP] as usize..ssp as usize].copy_from_slice(data);
            self.registers[REG_SSP] = ssp;

            Ok(())
        }
    }

    pub const fn tx_mem_address() -> usize {
        HASH_SIZE // Tx ID
            + WORD_SIZE // Tx size
            + MAX_INPUTS as usize * (COLOR_SIZE + WORD_SIZE) // Color/Balance
                                                             // coin input pairs
    }

    pub const fn block_height(&self) -> u32 {
        // TODO fetch block height
        u32::MAX >> 1
    }

    pub fn set_color_balance(&mut self, color: Color, balance: Word) {
        self.color_balances.insert(color, balance);
    }

    pub fn color_balance(&self, color: &Color) -> Word {
        self.color_balances.get(color).copied().unwrap_or(0)
    }

    pub fn set_flag(&mut self, a: Word) {
        self.registers[REG_FLAG] = a;
    }

    pub fn clear_err(&mut self) {
        self.registers[REG_ERR] = 0;
    }

    pub fn set_err(&mut self) {
        self.registers[REG_ERR] = 1;
    }

    pub fn inc_pc(&mut self) -> bool {
        let (result, overflow) = self.registers[REG_PC].overflowing_add(4);

        self.registers[REG_PC] = result;

        !overflow
    }

    pub fn memory(&self) -> &[u8] {
        self.memory.as_slice()
    }

    pub const fn registers(&self) -> &[Word] {
        &self.registers
    }

    pub const fn is_external_context(&self) -> bool {
        self.registers[REG_FP] == 0
    }

    pub const fn is_unsafe_math(&self) -> bool {
        self.registers[REG_FLAG] & 0x01 == 0x01
    }

    pub const fn is_wrapping(&self) -> bool {
        self.registers[REG_FLAG] & 0x02 == 0x02
    }

    pub const fn is_valid_register_alu(ra: RegisterId) -> bool {
        ra > REG_FLAG && ra < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_couple_alu(ra: RegisterId, rb: RegisterId) -> bool {
        ra > REG_FLAG && ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_triple_alu(ra: RegisterId, rb: RegisterId, rc: RegisterId) -> bool {
        ra > REG_FLAG && ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT && rc < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_quadruple_alu(
        ra: RegisterId,
        rb: RegisterId,
        rc: RegisterId,
        rd: RegisterId,
    ) -> bool {
        ra > REG_FLAG
            && ra < VM_REGISTER_COUNT
            && rb < VM_REGISTER_COUNT
            && rc < VM_REGISTER_COUNT
            && rd < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_quadruple(ra: RegisterId, rb: RegisterId, rc: RegisterId, rd: RegisterId) -> bool {
        ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT && rc < VM_REGISTER_COUNT && rd < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_triple(ra: RegisterId, rb: RegisterId, rc: RegisterId) -> bool {
        ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT && rc < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_couple(ra: RegisterId, rb: RegisterId) -> bool {
        ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register(ra: RegisterId) -> bool {
        ra < VM_REGISTER_COUNT
    }
}
