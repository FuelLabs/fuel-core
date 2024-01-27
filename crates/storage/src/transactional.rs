//! The primitives to work with storage in transactional mode.

use crate::Result as StorageResult;

#[cfg_attr(feature = "test-helpers", mockall::automock(type Storage = crate::test_helpers::EmptyStorage;))]
/// The types is transactional and may create `StorageTransaction`.
pub trait Transactional {
    /// The storage used when creating the transaction.
    type Storage: ?Sized;
    /// Creates and returns the storage transaction.
    fn transaction(&self) -> StorageTransaction<Self::Storage>;
}

/// The type is storage transaction and holds uncommitted state.
pub trait Transaction<Storage: ?Sized>:
    AsRef<Storage> + AsMut<Storage> + Send + Sync
{
    /// Commits the pending state changes into the storage.
    fn commit(&mut self) -> StorageResult<()>;
}

/// The storage transaction for the `Storage` type.
pub struct StorageTransaction<Storage: ?Sized> {
    transaction: Box<dyn Transaction<Storage>>,
}

impl<Storage> StorageTransaction<Storage> {
    /// Create a new storage transaction.
    pub fn new<T: Transaction<Storage> + 'static>(t: T) -> Self {
        Self {
            transaction: Box::new(t),
        }
    }
}

impl<Storage: Transactional + ?Sized> Transactional for StorageTransaction<Storage> {
    type Storage = Storage::Storage;

    fn transaction(&self) -> StorageTransaction<Self::Storage> {
        self.as_ref().transaction()
    }
}

impl<Storage: ?Sized> Transaction<Storage> for StorageTransaction<Storage> {
    fn commit(&mut self) -> StorageResult<()> {
        self.transaction.commit()
    }
}

impl<Storage: ?Sized + core::fmt::Debug> core::fmt::Debug
    for StorageTransaction<Storage>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StorageTransaction")
            .field("database", &self.transaction.as_ref().as_ref())
            .finish()
    }
}

impl<Storage: ?Sized> AsRef<Storage> for StorageTransaction<Storage> {
    fn as_ref(&self) -> &Storage {
        (*self.transaction).as_ref()
    }
}

impl<Storage: ?Sized> AsMut<Storage> for StorageTransaction<Storage> {
    fn as_mut(&mut self) -> &mut Storage {
        (*self.transaction).as_mut()
    }
}

impl<Storage: ?Sized> StorageTransaction<Storage> {
    /// Committing of the state consumes `Self`.
    pub fn commit(mut self) -> StorageResult<()> {
        self.transaction.commit()
    }
}

/// Provides a view of the storage at the given height.
/// It guarantees to be atomic, meaning the view is immutable to outside modifications.
pub trait AtomicView: Send + Sync {
    /// The type of the storage view.
    type View;

    /// The type used by the storage to track the commitments at a specific height.
    type Height;

    /// Returns the latest block height.
    fn latest_height(&self) -> Self::Height;

    /// Returns the view of the storage at the given `height`.
    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::View>;

    /// Returns the view of the storage for the latest block height.
    fn latest_view(&self) -> Self::View;
}
