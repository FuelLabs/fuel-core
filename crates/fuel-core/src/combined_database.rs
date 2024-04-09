use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
        },
        Database,
        Result as DatabaseResult,
    },
    service::DbType,
};
#[cfg(feature = "test-helpers")]
use fuel_core_chain_config::{
    StateConfig,
    StateConfigBuilder,
};
#[cfg(feature = "test-helpers")]
use fuel_core_storage::tables::{
    Coins,
    ContractsAssets,
    ContractsLatestUtxo,
    ContractsRawCode,
    ContractsState,
    Messages,
};
use fuel_core_storage::Result as StorageResult;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombinedDatabaseConfig {
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub max_database_cache_size: usize,
}

/// A database that combines the on-chain, off-chain and relayer databases into one entity.
#[derive(Default, Clone)]
pub struct CombinedDatabase {
    on_chain: Database<OnChain>,
    off_chain: Database<OffChain>,
    relayer: Database<Relayer>,
}

impl CombinedDatabase {
    pub fn new(
        on_chain: Database<OnChain>,
        off_chain: Database<OffChain>,
        relayer: Database<Relayer>,
    ) -> Self {
        Self {
            on_chain,
            off_chain,
            relayer,
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn prune(path: &std::path::Path) -> crate::database::Result<()> {
        crate::state::rocks_db::RocksDb::<OnChain>::prune(path)?;
        crate::state::rocks_db::RocksDb::<OffChain>::prune(path)?;
        crate::state::rocks_db::RocksDb::<Relayer>::prune(path)?;
        Ok(())
    }

    #[cfg(feature = "rocksdb")]
    pub fn open(
        path: &std::path::Path,
        capacity: usize,
    ) -> crate::database::Result<Self> {
        // TODO: Use different cache sizes for different databases
        let on_chain = Database::open_rocksdb(path, capacity)?;
        let off_chain = Database::open_rocksdb(path, capacity)?;
        let relayer = Database::open_rocksdb(path, capacity)?;
        Ok(Self {
            on_chain,
            off_chain,
            relayer,
        })
    }

    pub fn from_config(config: &CombinedDatabaseConfig) -> DatabaseResult<Self> {
        let combined_database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => {
                // use a default tmp rocksdb if no path is provided
                if config.database_path.as_os_str().is_empty() {
                    tracing::warn!(
                        "No RocksDB path configured, initializing database with a tmp directory"
                    );
                    CombinedDatabase::default()
                } else {
                    tracing::info!(
                        "Opening database {:?} with cache size \"{}\"",
                        config.database_path,
                        config.max_database_cache_size
                    );
                    CombinedDatabase::open(
                        &config.database_path,
                        config.max_database_cache_size,
                    )?
                }
            }
            DbType::InMemory => CombinedDatabase::in_memory(),
            #[cfg(not(feature = "rocksdb"))]
            _ => CombinedDatabase::in_memory(),
        };

        Ok(combined_database)
    }

    pub fn in_memory() -> Self {
        Self::new(
            Database::in_memory(),
            Database::in_memory(),
            Database::in_memory(),
        )
    }

    pub fn check_version(&self) -> StorageResult<()> {
        self.on_chain.check_version()?;
        self.off_chain.check_version()?;
        self.relayer.check_version()?;
        Ok(())
    }

    pub fn on_chain(&self) -> &Database<OnChain> {
        &self.on_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn on_chain_mut(&mut self) -> &mut Database<OnChain> {
        &mut self.on_chain
    }

    pub fn off_chain(&self) -> &Database<OffChain> {
        &self.off_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn off_chain_mut(&mut self) -> &mut Database<OffChain> {
        &mut self.off_chain
    }

    pub fn relayer(&self) -> &Database<Relayer> {
        &self.relayer
    }

    #[cfg(feature = "test-helpers")]
    pub fn read_state_config(&self) -> StorageResult<StateConfig> {
        use fuel_core_chain_config::AddTable;
        use itertools::Itertools;
        let mut builder = StateConfigBuilder::default();

        macro_rules! add_tables {
            ($($table: ty),*) => {
                $(
                    let table = self
                        .on_chain()
                        .entries::<$table>(None, fuel_core_storage::iter::IterDirection::Forward)
                        .try_collect()?;
                    builder.add(table);
                )*
            };
        }

        add_tables!(
            Coins,
            Messages,
            ContractsAssets,
            ContractsState,
            ContractsRawCode,
            ContractsLatestUtxo
        );

        let block = self.on_chain().latest_block()?;
        let state_config =
            builder.build(*block.header().height(), block.header().da_height)?;

        Ok(state_config)
    }
}
