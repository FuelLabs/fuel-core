use std::path::Path;

use fuel_core::{
    database::{
        self,
        database_description::DatabaseDescription,
        Database,
    },
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::DatabaseConfig,
    },
};
use fuel_core_global_merkle_root_api::ports::GetStateRoot;
use fuel_core_global_merkle_root_storage::{
    column::TableColumn,
    compute::ComputeStateRoot,
    merkle::MerkleMetadata,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable as _,
    },
    kv_store::{
        KeyValueInspect,
        Value,
    },
    merkle::column::MerkleizedColumn,
    transactional::{
        Changes,
        HistoricalView,
        Modifiable,
        ReadTransaction,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
};
use itertools::Itertools as _;

#[derive(Clone)]
/// State root database.
pub struct StateRootDb(Database<StateRootDbDescription>);

impl StateRootDb {
    /// Create an in-memory database.
    pub fn in_memory() -> Self {
        Self(Database::in_memory())
    }

    /// Open a rocksdb database.
    pub fn open_rocksdb(
        path: &Path,
        state_rewind_policy: StateRewindPolicy,
        database_config: DatabaseConfig,
    ) -> Result<Self, database::Error> {
        Ok(Self(Database::open_rocksdb(
            path,
            state_rewind_policy,
            database_config,
        )?))
    }

    /// Get the latest block height from the database metadata.
    pub fn block_height(&self) -> StorageResult<BlockHeight> {
        Ok(self.0.latest_height_from_metadata()?.unwrap_or_default())
    }
}

/// Database description for the state root DB.
#[derive(Clone, Copy, Debug)]
pub struct StateRootDbDescription;

impl DatabaseDescription for StateRootDbDescription {
    type Column = MerkleizedColumn<TableColumn>;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        String::from("state_root")
    }

    fn metadata_column() -> Self::Column {
        Self::Column::MerkleMetadataColumn
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}

impl GetStateRoot for StateRootDb {
    fn state_root_at(&self, height: BlockHeight) -> StorageResult<Bytes32> {
        self.0.view_at(&height)?.read_transaction().state_root()
    }
}

impl Modifiable for StateRootDb {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        fuel_core::database::commit_changes_with_height_update(
            &mut self.0,
            changes,
            |iter| {
                iter.iter_all_keys::<MerkleMetadata>(Some(IterDirection::Reverse))
                    .map(|res| res.map(BlockHeight::from))
                    .try_collect()
            },
        )
    }
}

impl KeyValueInspect for StateRootDb {
    type Column = MerkleizedColumn<TableColumn>;

    #[doc = " Returns the value from the storage."]
    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.0.get(key, column)
    }
}
