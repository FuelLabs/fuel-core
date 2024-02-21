use crate::{
    database::database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
        DatabaseDescription,
    },
    state::{
        in_memory::memory_store::MemoryStore,
        DataSource,
    },
};
use fuel_core_chain_config::{
    ChainConfigDb,
    CoinConfig,
    ContractConfig,
    MessageConfig,
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    iter::IterDirection,
    kv_store::{
        KeyValueInspect,
        Value,
    },
    structured_storage::TableWithBlueprint,
    transactional::{
        AtomicView,
        Changes,
        Modifiable,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

pub use fuel_core_database::Error;
pub type Result<T> = core::result::Result<T, Error>;

// TODO: Extract `Database` and all belongs into `fuel-core-database`.
#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
#[cfg(feature = "rocksdb")]
use std::path::Path;

// Storages implementation
pub mod balances;
pub mod block;
pub mod coin;
pub mod contracts;
pub mod database_description;
pub mod message;
pub mod metadata;
pub mod sealed_block;
pub mod state;
pub mod storage;
pub mod transactions;

#[derive(Clone, Debug)]
pub struct Database<Description = OnChain>
where
    Description: DatabaseDescription,
{
    data: DataSource<Description>,
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(data_source: DataSource<Description>) -> Self {
        Self { data: data_source }
    }

    #[cfg(feature = "rocksdb")]
    pub fn open(path: &Path, capacity: impl Into<Option<usize>>) -> Result<Self> {
        use anyhow::Context;
        let db = RocksDb::<Description>::default_open(path, capacity.into()).map_err(Into::<anyhow::Error>::into).context("Failed to open rocksdb, you may need to wipe a pre-existing incompatible db `rm -rf ~/.fuel/db`")?;

        Ok(Database { data: Arc::new(db) })
    }

    pub fn in_memory() -> Self {
        let data = Arc::<MemoryStore<Description>>::new(MemoryStore::default());
        Self { data }
    }

    #[cfg(feature = "rocksdb")]
    pub fn rocksdb() -> Self {
        let data =
            Arc::<RocksDb<Description>>::new(RocksDb::default_open_temp(None).unwrap());
        Self { data }
    }
}

impl<Description> KeyValueInspect for Database<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.data.as_ref().exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.data.as_ref().size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.data.as_ref().get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.data.as_ref().read(key, column, buf)
    }
}

impl<Description> Modifiable for Database<Description>
where
    Description: DatabaseDescription,
{
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.data.as_ref().commit_changes(changes)
    }
}

/// Read-only methods.
impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub(crate) fn iter_all<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: BlueprintInspect<M, Self>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, None, direction)
    }

    pub(crate) fn iter_all_by_prefix<M, P>(
        &self,
        prefix: Option<P>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: BlueprintInspect<M, Self>,
        P: AsRef<[u8]>,
    {
        self.iter_all_filtered::<M, P>(prefix, None, None)
    }

    pub(crate) fn iter_all_by_start<M>(
        &self,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: BlueprintInspect<M, Self>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, start, direction)
    }

    pub(crate) fn iter_all_filtered<M, P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(M::OwnedKey, M::OwnedValue)>> + '_
    where
        M: Mappable + TableWithBlueprint<Column = Description::Column>,
        M::Blueprint: BlueprintInspect<M, Self>,
        P: AsRef<[u8]>,
    {
        let encoder = start.map(|start| {
            <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::encode(start)
        });

        let start = encoder.as_ref().map(|encoder| encoder.as_bytes());

        self.data
            .as_ref()
            .iter_all(
                M::column(),
                prefix.as_ref().map(|p| p.as_ref()),
                start.as_ref().map(|cow| cow.as_ref()),
                direction.unwrap_or_default(),
            )
            .map(|val| {
                val.and_then(|(key, value)| {
                    let key =
                        <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::decode(
                            key.as_slice(),
                        )
                        .map_err(|e| StorageError::Codec(anyhow::anyhow!(e)))?;
                    let value =
                        <M::Blueprint as BlueprintInspect<M, Self>>::ValueCodec::decode(
                            value.as_slice(),
                        )
                        .map_err(|e| StorageError::Codec(anyhow::anyhow!(e)))?;
                    Ok((key, value))
                })
            })
    }
}

impl<Description> AsRef<Database<Description>> for Database<Description>
where
    Description: DatabaseDescription,
{
    fn as_ref(&self) -> &Database<Description> {
        self
    }
}

impl<Description> AsMut<Database<Description>> for Database<Description>
where
    Description: DatabaseDescription,
{
    fn as_mut(&mut self) -> &mut Database<Description> {
        self
    }
}

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl<Description> Default for Database<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        #[cfg(not(feature = "rocksdb"))]
        {
            Self::in_memory()
        }
        #[cfg(feature = "rocksdb")]
        {
            Self::rocksdb()
        }
    }
}

/// Implement `ChainConfigDb` so that `Database` can be passed to
/// `StateConfig's` `generate_state_config()` method
impl ChainConfigDb for Database {
    fn get_coin_config(&self) -> StorageResult<Option<Vec<CoinConfig>>> {
        Self::get_coin_config(self).map_err(Into::into)
    }

    fn get_contract_config(&self) -> StorageResult<Option<Vec<ContractConfig>>> {
        Self::get_contract_config(self)
    }

    fn get_message_config(&self) -> StorageResult<Option<Vec<MessageConfig>>> {
        Self::get_message_config(self).map_err(Into::into)
    }

    fn get_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }
}

impl AtomicView for Database<OnChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> BlockHeight {
        // TODO: The database should track the latest height inside of the database object
        //  instead of fetching it from the `FuelBlocks` table. As a temporary solution,
        //  fetch it from the table for now.
        self.latest_height().unwrap_or_default()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<OffChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> BlockHeight {
        // TODO: The database should track the latest height inside of the database object
        //  instead of fetching it from the `FuelBlocks` table. As a temporary solution,
        //  fetch it from the table for now.
        self.latest_height().unwrap_or_default()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<Relayer> {
    type View = Self;
    type Height = DaBlockHeight;

    fn latest_height(&self) -> Self::Height {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            // TODO: The database should track the latest da height inside of the database object
            //  instead of fetching it from the `RelayerMetadata` table. As a temporary solution,
            //  fetch it from the table for now.
            //  https://github.com/FuelLabs/fuel-core/issues/1589
            self.get_finalized_da_height().unwrap_or_default()
        }
        #[cfg(not(feature = "relayer"))]
        {
            DaBlockHeight(0)
        }
    }

    fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        self.clone()
    }
}

#[cfg(feature = "rocksdb")]
pub fn convert_to_rocksdb_direction(direction: IterDirection) -> rocksdb::Direction {
    match direction {
        IterDirection::Forward => rocksdb::Direction::Forward,
        IterDirection::Reverse => rocksdb::Direction::Reverse,
    }
}

#[cfg(test)]
mod tests {
    use crate::database::database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
        DatabaseDescription,
    };

    fn column_keys_not_exceed_count<Description>()
    where
        Description: DatabaseDescription,
    {
        use enum_iterator::all;
        use fuel_core_storage::kv_store::StorageColumn;
        use strum::EnumCount;
        for column in all::<Description::Column>() {
            assert!(column.as_usize() < Description::Column::COUNT);
        }
    }

    #[test]
    fn column_keys_not_exceed_count_test_on_chain() {
        column_keys_not_exceed_count::<OnChain>();
    }

    #[test]
    fn column_keys_not_exceed_count_test_off_chain() {
        column_keys_not_exceed_count::<OffChain>();
    }

    #[test]
    fn column_keys_not_exceed_count_test_relayer() {
        column_keys_not_exceed_count::<Relayer>();
    }
}
