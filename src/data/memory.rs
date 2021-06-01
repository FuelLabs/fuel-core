use super::{DataError, Storage};
use crate::interpreter::Contract;

use fuel_asm::Word;
use fuel_tx::{Color, ContractAddress};

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct MemoryStorage {
    contracts: HashMap<ContractAddress, Contract>,
    color_balances: HashMap<Color, Word>,
}

impl Storage<ContractAddress, Contract> for MemoryStorage {
    fn insert(&mut self, key: ContractAddress, value: Contract) -> Result<Option<Contract>, DataError> {
        Ok(self.contracts.insert(key, value))
    }

    fn remove(&mut self, key: &ContractAddress) -> Result<Option<Contract>, DataError> {
        Ok(self.contracts.remove(key))
    }

    fn get(&self, key: &ContractAddress) -> Result<Option<Contract>, DataError> {
        Ok(self.contracts.get(key).cloned())
    }

    fn contains_key(&self, key: &ContractAddress) -> Result<bool, DataError> {
        Ok(self.contracts.contains_key(key))
    }
}

impl Storage<Color, Word> for MemoryStorage {
    fn insert(&mut self, key: Color, value: Word) -> Result<Option<Word>, DataError> {
        Ok(self.color_balances.insert(key, value))
    }

    fn get(&self, key: &Color) -> Result<Option<Word>, DataError> {
        Ok(self.color_balances.get(key).copied())
    }

    fn remove(&mut self, key: &Color) -> Result<Option<Word>, DataError> {
        Ok(self.color_balances.remove(key))
    }

    fn contains_key(&self, key: &Color) -> Result<bool, DataError> {
        Ok(self.color_balances.contains_key(key))
    }
}
