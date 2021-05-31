use crate::interpreter::Contract;

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

    fn get(&self, key: &K) -> Result<Option<&V>, DataError>;
    fn contains_key(&self, key: &K) -> Result<bool, DataError>;
}

// Provisory implementation that will cover ID definitions until client backend
// is implemented
impl Key for [u8; 32] {}
impl Value for u64 {}
impl Value for Contract {}
