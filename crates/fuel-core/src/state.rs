use crate::database::{
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
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

pub type DataSource = Arc<dyn TransactableStorage<Column = Column>>;

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

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
