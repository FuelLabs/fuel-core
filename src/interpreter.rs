use crate::consts::*;

use fuel_asm::{RegisterId, Word};
use fuel_tx::consts::*;
use fuel_tx::{Color, Hash, Transaction};

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

const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Clone)]
pub struct Interpreter<S> {
    registers: [Word; VM_REGISTER_COUNT],
    memory: Vec<u8>,
    frames: Vec<CallFrame>,
    log: Vec<LogEvent>,
    // TODO review all opcodes that mutates the tx in the stack and keep this one sync
    tx: Transaction,
    storage: S,
}

impl<S> Interpreter<S> {
    pub fn with_storage(storage: S) -> Self {
        Self {
            registers: [0; VM_REGISTER_COUNT],
            memory: vec![0; VM_MAX_RAM as usize],
            frames: vec![],
            log: vec![],
            tx: Transaction::default(),
            storage,
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
        Hash::size_of() // Tx ID
            + WORD_SIZE // Tx size
            + MAX_INPUTS as usize * (Color::size_of() + WORD_SIZE) // Color/Balance
                                                                   // coin input
                                                                   // pairs
    }

    pub const fn block_height(&self) -> u32 {
        // TODO fetch block height
        u32::MAX >> 1
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
