//! The primitives to work with storage in transactional mode.

use crate::Result as StorageResult;

/// The type is transactional and holds uncommitted state.
pub trait Transactional<Storage>: AsRef<Storage> + AsMut<Storage> + Send + Sync {
    /// Commits the pending state changes into the storage.
    fn commit(&mut self) -> StorageResult<()>;
}

/// The storage transaction for the `Storage` type.
pub struct StorageTransaction<Storage> {
    transaction: Box<dyn Transactional<Storage>>,
}

impl<Storage> StorageTransaction<Storage> {
    /// Create a new storage transaction.
    pub fn new<T: Transactional<Storage> + 'static>(t: T) -> Self {
        Self {
            transaction: Box::new(t),
        }
    }
}

impl<Storage> Transactional<Storage> for StorageTransaction<Storage> {
    fn commit(&mut self) -> StorageResult<()> {
        self.transaction.commit()
    }
}

impl<Storage: core::fmt::Debug> core::fmt::Debug for StorageTransaction<Storage> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StorageTransaction")
            .field("database", (*self.transaction).as_ref())
            .finish()
    }
}

impl<Storage> AsRef<Storage> for StorageTransaction<Storage> {
    fn as_ref(&self) -> &Storage {
        (*self.transaction).as_ref()
    }
}

impl<Storage> AsMut<Storage> for StorageTransaction<Storage> {
    fn as_mut(&mut self) -> &mut Storage {
        (*self.transaction).as_mut()
    }
}

impl<Storage> StorageTransaction<Storage> {
    /// Committing of the state consumes `Self`.
    pub fn commit(mut self) -> StorageResult<()> {
        self.transaction.commit()
    }
}
