use crate::interpreter::Contract;

use fuel_asm::Word;
use fuel_tx::{Color, ContractAddress};

mod error;
mod memory;

pub use error::DataError;
pub use memory::MemoryStorage;

pub trait Key {}

pub trait Value {}

pub trait Storage<K, V>
where
    K: Key,
    V: Value,
{
    fn insert(&mut self, key: K, value: V) -> Result<Option<V>, DataError>;
    fn remove(&mut self, key: &K) -> Result<Option<V>, DataError>;

    // This initial implementation safeguard from the complex scenarios when a
    // reference is returned. To simplify, at least for now, we return the owned
    // value.
    fn get(&self, key: &K) -> Result<Option<V>, DataError>;
    fn contains_key(&self, key: &K) -> Result<bool, DataError>;
}

// Provisory implementation that will cover ID definitions until client backend
// is implemented
impl Key for Color {}
impl Key for ContractAddress {}
impl Value for Word {}
impl Value for Contract {}
