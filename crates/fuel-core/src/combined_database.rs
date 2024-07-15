use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
        },
        Database,
        GenesisDatabase,
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

#[cfg(feature = "rocksdb")]
use crate::state::historical_rocksdb::StateRewindPolicy;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombinedDatabaseConfig {
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub max_database_cache_size: usize,
    #[cfg(feature = "rocksdb")]
    pub state_rewind_policy: StateRewindPolicy,
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
        state_rewind_policy: StateRewindPolicy,
    ) -> crate::database::Result<Self> {
        // TODO: Use different cache sizes for different databases
        let on_chain = Database::open_rocksdb(path, capacity, state_rewind_policy)?;
        let off_chain = Database::open_rocksdb(path, capacity, state_rewind_policy)?;
        let relayer =
            Database::open_rocksdb(path, capacity, StateRewindPolicy::NoRewind)?;
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
                        "Opening database {:?} with cache size \"{}\" and state rewind policy \"{:?}\"",
                        config.database_path,
                        config.max_database_cache_size,
                        config.state_rewind_policy,
                    );
                    CombinedDatabase::open(
                        &config.database_path,
                        config.max_database_cache_size,
                        config.state_rewind_policy,
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

    #[cfg(any(feature = "test-helpers", test))]
    pub fn relayer_mut(&mut self) -> &mut Database<Relayer> {
        &mut self.relayer
    }

    #[cfg(feature = "test-helpers")]
    pub fn read_state_config(&self) -> StorageResult<StateConfig> {
        use fuel_core_chain_config::AddTable;
        use fuel_core_producer::ports::BlockProducerDatabase;
        use fuel_core_storage::transactional::AtomicView;
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

        let view = self.on_chain().latest_view()?;
        let latest_block = view.latest_block()?;
        let blocks_root =
            view.block_header_merkle_root(latest_block.header().height())?;
        let state_config =
            builder.build(Some(fuel_core_chain_config::LastBlockConfig::from_header(
                latest_block.header(),
                blocks_root,
            )))?;

        Ok(state_config)
    }

    /// Converts the combined database into a genesis combined database.
    pub fn into_genesis(self) -> CombinedGenesisDatabase {
        CombinedGenesisDatabase {
            on_chain: self.on_chain.into_genesis(),
            off_chain: self.off_chain.into_genesis(),
            relayer: self.relayer.into_genesis(),
        }
    }
}

/// A genesis database that combines the on-chain, off-chain and relayer
/// genesis databases into one entity.
#[derive(Default, Clone)]
pub struct CombinedGenesisDatabase {
    on_chain: GenesisDatabase<OnChain>,
    off_chain: GenesisDatabase<OffChain>,
    relayer: GenesisDatabase<Relayer>,
}

impl CombinedGenesisDatabase {
    pub fn on_chain(&self) -> &GenesisDatabase<OnChain> {
        &self.on_chain
    }

    pub fn off_chain(&self) -> &GenesisDatabase<OffChain> {
        &self.off_chain
    }

    pub fn relayer(&self) -> &GenesisDatabase<Relayer> {
        &self.relayer
    }
}
