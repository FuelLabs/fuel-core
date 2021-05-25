use super::Interpreter;
use crate::{consts, crypto};

use fuel_tx::crypto as tx_crypto;
use fuel_tx::{ContractAddress, Transaction, ValidationError};

use std::convert::TryFrom;

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
        let mut input = consts::VM_CONTRACT_ID_BASE.to_vec();

        input.extend_from_slice(salt);
        input.extend_from_slice(&crypto::merkle_root(self.0.as_slice()));

        tx_crypto::hash(input.as_slice())
    }
}

impl Interpreter {
    pub fn contract(&self, address: &ContractAddress) -> Option<&Contract> {
        self.contracts.get(address)
    }

    pub fn check_contract_exists(&self, address: &ContractAddress) -> bool {
        self.contracts.contains_key(address)
    }
}
