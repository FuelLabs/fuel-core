#[cfg(feature = "default")]
use crate::database::columns::COLUMN_NUM;
use crate::database::columns::{BALANCES, CONTRACTS, CONTRACTS_CODE_ROOT, CONTRACTS_STATE};
use crate::database::{Database, DatabaseTrait};
use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::in_memory::transaction::MemoryTransactionView;
#[cfg(feature = "default")]
use crate::state::rocks_db::RocksDb;
use crate::state::{ColumnId, DataSource, Error, MultiKey};
use fuel_vm::crypto;
use fuel_vm::data::{DataError, InterpreterStorage, MerkleStorage};
use fuel_vm::prelude::{Address, Bytes32, Color, Contract, ContractId, Salt, Storage, Word};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct DatabaseTransaction {
    // The primary datastores
    changes: Arc<MemoryTransactionView>,
    // The inner db impl using these stores
    database: Database,
}

impl AsRef<Database> for DatabaseTransaction {
    fn as_ref(&self) -> &Database {
        &self.database
    }
}

impl DerefMut for DatabaseTransaction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.database
    }
}

impl Deref for DatabaseTransaction {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

impl Default for DatabaseTransaction {
    fn default() -> Self {
        Database::default().transaction()
    }
}

impl DatabaseTransaction {
    pub fn commit(self) -> crate::state::Result<()> {
        // TODO: should commit be fallible if this api is meant to be atomic?
        self.changes.commit()
    }
}

impl From<&Database> for DatabaseTransaction {
    fn from(source: &Database) -> Self {
        let data = Arc::new(MemoryTransactionView::new(source.data.clone()));
        Self {
            changes: data.clone(),
            database: Database { data },
        }
    }
}

impl DatabaseTrait for DatabaseTransaction {
    fn transaction(&self) -> DatabaseTransaction {
        (&self.database).into()
    }
}
