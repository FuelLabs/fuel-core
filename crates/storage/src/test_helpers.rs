//! The module to help with tests.

use crate::{
    transactional::{
        StorageTransaction,
        Transaction,
        Transactional,
    },
    Result,
};

/// The empty transactional storage.
#[derive(Default, Clone, Copy)]
pub struct EmptyStorage;

impl AsRef<EmptyStorage> for EmptyStorage {
    fn as_ref(&self) -> &EmptyStorage {
        self
    }
}

impl AsMut<EmptyStorage> for EmptyStorage {
    fn as_mut(&mut self) -> &mut EmptyStorage {
        self
    }
}

impl Transactional<EmptyStorage> for EmptyStorage {
    fn transaction(&self) -> StorageTransaction<EmptyStorage> {
        StorageTransaction::new(EmptyStorage)
    }
}

impl Transaction<EmptyStorage> for EmptyStorage {
    fn commit(&mut self) -> Result<()> {
        Ok(())
    }
}
