use crate::{
    database::{
        database_description::DatabaseDescription,
        Database,
    },
    state::{
        in_memory::transaction::MemoryTransactionView,
        DataSource,
    },
};
use fuel_core_storage::{
    transactional::Transaction,
    Result as StorageResult,
};
use std::{
    fmt::Debug,
    ops::{
        Deref,
        DerefMut,
    },
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    // The primary datastores
    changes: Arc<MemoryTransactionView<Description>>,
    // The inner db impl using these stores
    database: Database<Description>,
}

impl<Description> AsRef<Database<Description>> for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn as_ref(&self) -> &Database<Description> {
        &self.database
    }
}

impl<Description> AsMut<Database<Description>> for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn as_mut(&mut self) -> &mut Database<Description> {
        &mut self.database
    }
}

impl<Description> Deref for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    type Target = Database<Description>;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

impl<Description> DerefMut for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.database
    }
}

impl<Description> Default for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        Database::<Description>::default().transaction()
    }
}

impl<Description> Transaction<Database<Description>> for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn commit(&mut self) -> StorageResult<()> {
        // TODO: should commit be fallible if this api is meant to be atomic?
        self.changes.commit()
    }
}

impl<Description> From<&Database<Description>> for DatabaseTransaction<Description>
where
    Description: DatabaseDescription,
{
    fn from(source: &Database<Description>) -> Self {
        let database: &DataSource<Description> = source.data.as_ref();
        let data = Arc::new(MemoryTransactionView::new(database.clone()));
        Self {
            changes: data.clone(),
            database: Database::<Description>::new(data),
        }
    }
}
