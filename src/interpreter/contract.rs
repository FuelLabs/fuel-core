use super::{ExecuteError, Interpreter, MemoryRange};
use crate::consts::*;
use crate::crypto;
use crate::data::Storage;

use fuel_asm::{Opcode, Word};
use fuel_tx::bytes::{SerializableVec, SizedBytes};
use fuel_tx::consts::*;
use fuel_tx::crypto as tx_crypto;
use fuel_tx::{Color, ContractAddress, Input, Output, Transaction, ValidationError};
use itertools::Itertools;

use std::convert::TryFrom;
use std::mem;

const COLOR_SIZE: usize = mem::size_of::<Color>();
const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Contract(Vec<u8>);

impl From<Vec<u8>> for Contract {
    fn from(c: Vec<u8>) -> Self {
        Self(c)
    }
}

impl From<&[u8]> for Contract {
    fn from(c: &[u8]) -> Self {
        Self(c.into())
    }
}

impl From<&mut [u8]> for Contract {
    fn from(c: &mut [u8]) -> Self {
        Self(c.into())
    }
}

impl From<Contract> for Vec<u8> {
    fn from(c: Contract) -> Vec<u8> {
        c.0
    }
}

impl AsRef<[u8]> for Contract {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for Contract {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

impl TryFrom<&Transaction> for Contract {
    type Error = ValidationError;

    fn try_from(tx: &Transaction) -> Result<Self, Self::Error> {
        match tx {
            Transaction::Create {
                bytecode_witness_index,
                witnesses,
                ..
            } => witnesses
                .get(*bytecode_witness_index as usize)
                .map(|c| c.as_ref().into())
                .ok_or(ValidationError::TransactionCreateBytecodeWitnessIndex),

            _ => Err(ValidationError::TransactionScriptOutputContractCreated { index: 0 }),
        }
    }
}

impl Contract {
    pub fn address(&self, salt: &[u8]) -> ContractAddress {
        let mut input = VM_CONTRACT_ID_BASE.to_vec();

        input.extend_from_slice(salt);
        input.extend_from_slice(&crypto::merkle_root(self.0.as_slice()));

        tx_crypto::hash(input.as_slice())
    }
}

impl<S> Interpreter<S>
where
    S: Storage<ContractAddress, Contract> + Storage<Color, Word>,
{
    pub fn contract(&self, address: &ContractAddress) -> Result<Option<&Contract>, ExecuteError> {
        Ok(self.storage.get(address)?)
    }

    pub fn check_contract_exists(&self, address: &ContractAddress) -> Result<bool, ExecuteError> {
        Ok(<S as Storage<ContractAddress, Contract>>::contains_key(
            &self.storage,
            address,
        )?)
    }

    pub fn set_color_balance(&mut self, color: Color, balance: Word) -> Result<(), ExecuteError> {
        self.storage.insert(color, balance)?;

        Ok(())
    }

    pub fn color_balance(&self, color: &Color) -> Result<Word, ExecuteError> {
        Ok(self.storage.get(color)?.copied().unwrap_or(0))
    }

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
                .try_for_each::<_, Result<_, ExecuteError>>(|color| {
                    let balance = self.color_balance(color)?;

                    self.memory[ssp..ssp + COLOR_SIZE].copy_from_slice(color);
                    ssp += COLOR_SIZE;

                    self.memory[ssp..ssp + WORD_SIZE].copy_from_slice(&balance.to_be_bytes());
                    ssp += WORD_SIZE;

                    Ok(())
                })?;
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
                if static_contracts
                    .iter()
                    .any(|id| !self.check_contract_exists(id).unwrap_or(false))
                {
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

                self.storage.insert(id, contract)?;

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
}
