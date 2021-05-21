use crate::consts::*;
use crate::crypto::hash;

use fuel_asm::{RegisterId, Word};
use fuel_tx::consts::*;
use fuel_tx::{Color, ContractAddress, Input, Output, Transaction, ValidationError};

use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Read;
use std::mem;

mod alu;
mod contract;
mod crypto;
mod error;
mod execution;
mod flow;
mod frame;
mod log;
mod memory;

pub use contract::Contract;
pub use error::ExecuteError;
pub use frame::{Call, CallFrame};
pub use log::LogEvent;
pub use memory::MemoryRange;

#[derive(Debug, Clone)]
pub struct Interpreter {
    registers: [Word; VM_REGISTER_COUNT],
    memory: Vec<u8>,
    frames: Vec<CallFrame>,
    log: Vec<LogEvent>,
    // TODO review all opcodes that mutates the tx in the stack and keep this one sync
    tx: Transaction,
    contracts: HashMap<ContractAddress, Contract>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            registers: [0; VM_REGISTER_COUNT],
            memory: vec![],
            frames: vec![],
            log: vec![],
            tx: Transaction::default(),
            contracts: HashMap::new(),
        }
    }
}

impl Interpreter {
    pub fn init(&mut self, mut tx: Transaction) -> Result<(), ValidationError> {
        tx.validate(self.block_height() as Word)?;

        self.frames.clear();
        self.log.clear();

        self.registers.copy_from_slice(&[0; VM_REGISTER_COUNT]);
        self.memory = vec![0; VM_MAX_RAM as usize];

        self.registers[REG_ONE] = 1;
        self.registers[REG_SSP] = 0;

        // TODO push pairs of colors to stack
        let mut ssp = 0;
        for i in 0..(MAX_INPUTS as usize) {
            let (color, balance) = match tx.inputs().get(i) {
                Some(Input::Coin { color, .. }) => (color, 0),
                _ => (&[0; mem::size_of::<Color>()], 0u64),
            };

            self.memory[ssp..color.len() + ssp].copy_from_slice(color);
            self.memory[ssp + color.len()..color.len() + 8 + ssp].copy_from_slice(&balance.to_be_bytes());
            ssp += color.len() + 8;
        }

        let tx_stack = self.tx_stack();

        // Push tx len and bytes to stack
        let tx_len = tx.read(&mut self.memory[tx_stack..]).unwrap_or_else(|_| unreachable!());
        let tx_hash = hash(&self.memory[tx_stack..tx_stack + tx_len]);
        self.memory[32..40].copy_from_slice(&(tx_len as u64).to_be_bytes());
        self.memory[..32].copy_from_slice(&tx_hash);

        // TODO set current stack pointer
        self.registers[REG_SSP] = tx_stack as Word + tx_len as Word;
        self.registers[REG_SP] = self.registers[REG_SSP];

        // Set heap area
        self.registers[REG_FP] = VM_MAX_RAM - 1;
        self.registers[REG_HP] = self.registers[REG_FP];

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
                let offsets: Vec<usize> = tx
                    .inputs()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, input)| match input {
                        Input::Coin { predicate, .. } if !predicate.is_empty() => tx.input_coin_data_offset(i),
                        _ => None,
                    })
                    .collect();

                for offset in offsets {
                    self.registers[REG_PC] = offset as Word;
                    self.registers[REG_IS] = offset as Word;

                    self.verify_predicate()?;
                }

                Ok(())
            }

            _ => unimplemented!(),
        }
    }

    pub fn verify_predicate(&mut self) -> Result<(), ExecuteError> {
        // TODO
        Ok(())
    }

    pub const fn tx_stack(&self) -> usize {
        // TODO update tx address in stack according to specs
        40
    }

    pub const fn block_height(&self) -> u32 {
        // TODO fetch block height
        u32::MAX >> 1
    }

    pub fn set_flag(&mut self, a: Word) {
        self.registers[REG_FLAG] = a;
    }

    pub fn clear_err(&mut self) {
        self.registers[REG_ERR] = 1;
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

    pub const fn is_unsafe_math(&self) -> bool {
        self.registers[REG_FLAG] & 0x01 == 0x01
    }

    pub const fn is_wrapping(&self) -> bool {
        self.registers[REG_FLAG] & 0x02 == 0x02
    }

    pub const fn is_valid_register_triple_alu(ra: RegisterId, rb: RegisterId, rc: RegisterId) -> bool {
        ra > REG_FLAG && ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT && rc < VM_REGISTER_COUNT
    }

    pub const fn is_valid_register_couple_alu(ra: RegisterId, rb: RegisterId) -> bool {
        ra > REG_FLAG && ra < VM_REGISTER_COUNT && rb < VM_REGISTER_COUNT
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
