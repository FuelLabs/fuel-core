use crate::database::database_description::DatabaseDescription;
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorableStore,
    },
    transactional::Changes,
    Result as StorageResult,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;

#[allow(type_alias_bounds)]
pub type DataSource<Description>
where
    Description: DatabaseDescription,
= Arc<dyn TransactableStorage<Column = Description::Column>>;

pub trait TransactableStorage: IteratorableStore + Debug + Send + Sync {
    /// Commits the changes into the storage.
    fn commit_changes(&self, changes: Changes) -> StorageResult<()>;
}
