use crate::{
    database::{
        database_description::{
            on_chain::OnChain,
            DatabaseDescription,
        },
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

type DataSourceInner<Column> = Arc<dyn TransactableStorage<Column = Column>>;

#[derive(Clone, Debug)]
pub struct DataSource<Description = OnChain>(DataSourceInner<Description::Column>)
where
    Description: DatabaseDescription;

impl<Description> From<Arc<MemoryTransactionView<Description>>>
    for DataSource<Description>
where
    Description: DatabaseDescription,
{
    fn from(inner: Arc<MemoryTransactionView<Description>>) -> Self {
        Self(inner)
    }
}

#[cfg(feature = "rocksdb")]
impl<Description> From<Arc<rocks_db::RocksDb<Description>>> for DataSource<Description>
where
    Description: DatabaseDescription,
{
    fn from(inner: Arc<rocks_db::RocksDb<Description>>) -> Self {
        Self(inner)
    }
}

impl<Description> From<Arc<MemoryStore<Description>>> for DataSource<Description>
where
    Description: DatabaseDescription,
{
    fn from(inner: Arc<MemoryStore<Description>>) -> Self {
        Self(inner)
    }
}

impl<Description> core::ops::Deref for DataSource<Description>
where
    Description: DatabaseDescription,
{
    type Target = DataSourceInner<Description::Column>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Description> core::ops::DerefMut for DataSource<Description>
where
    Description: DatabaseDescription,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait TransactableStorage:
    IteratorableStore + BatchOperations + Debug + Send + Sync
{
    fn flush(&self) -> DatabaseResult<()>;
}
