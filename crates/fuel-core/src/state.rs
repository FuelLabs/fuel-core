use crate::{
    database::{
        Column,
        Database,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::in_memory::{
        memory_store::MemoryStore,
        transaction::MemoryTransactionView,
    },
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorableStore,
    },
    kv_store::BatchOperations,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;

type DataSourceInner = Arc<dyn TransactableStorage<Column = Column>>;

#[derive(Clone, Debug)]
pub struct DataSource(DataSourceInner);

impl From<Arc<MemoryTransactionView>> for DataSource {
    fn from(inner: Arc<MemoryTransactionView>) -> Self {
        Self(inner)
    }
}

#[cfg(feature = "rocksdb")]
impl From<Arc<rocks_db::RocksDb>> for DataSource {
    fn from(inner: Arc<rocks_db::RocksDb>) -> Self {
        Self(inner)
    }
}

impl From<Arc<MemoryStore>> for DataSource {
    fn from(inner: Arc<MemoryStore>) -> Self {
        Self(inner)
    }
}

impl core::ops::Deref for DataSource {
    type Target = DataSourceInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for DataSource {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait TransactableStorage:
    IteratorableStore + BatchOperations + Debug + Send + Sync
{
    fn checkpoint(&self) -> DatabaseResult<Database> {
        Err(DatabaseError::Other(anyhow::anyhow!(
            "Checkpoint is not supported"
        )))
    }

    fn flush(&self) -> DatabaseResult<()>;
}
