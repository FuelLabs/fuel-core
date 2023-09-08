use crate::database::{
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_chain_config::ChainConfig;

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
/// Tracks the total number of transactions written to the chain
/// It's useful for analyzing TPS or other metrics.
pub(crate) const TX_COUNT: &[u8] = b"total_tx_count";

/// Can be used to perform migrations in the future.
pub(crate) const DB_VERSION: u32 = 0x00;

impl Database {
    /// Ensures the database is initialized and that the database version is correct
    pub fn init(&self, config: &ChainConfig) -> DatabaseResult<()> {
        // initialize chain name if not set
        if self.get_chain_name()?.is_none() {
            self.insert(CHAIN_NAME_KEY, Column::Metadata, &config.chain_name)
                .and_then(|v: Option<String>| {
                    if v.is_some() {
                        Err(DatabaseError::ChainAlreadyInitialized)
                    } else {
                        Ok(())
                    }
                })?;
        }

        // Ensure the database version is correct
        if let Some(version) = self.get::<u32>(DB_VERSION_KEY, Column::Metadata)? {
            if version != DB_VERSION {
                return Err(DatabaseError::InvalidDatabaseVersion {
                    found: version,
                    expected: DB_VERSION,
                })?
            }
        } else {
            let _: Option<u32> =
                self.insert(DB_VERSION_KEY, Column::Metadata, &DB_VERSION)?;
        }
        Ok(())
    }

    pub fn get_chain_name(&self) -> DatabaseResult<Option<String>> {
        self.get(CHAIN_NAME_KEY, Column::Metadata)
    }

    pub fn increase_tx_count(&self, new_txs: u64) -> DatabaseResult<u64> {
        // TODO: how should tx count be initialized after regenesis?
        let current_tx_count: u64 =
            self.get(TX_COUNT, Column::Metadata)?.unwrap_or_default();
        // Using saturating_add because this value doesn't significantly impact the correctness of execution.
        let new_tx_count = current_tx_count.saturating_add(new_txs);
        self.insert::<_, _, u64>(TX_COUNT, Column::Metadata, &new_tx_count)?;
        Ok(new_tx_count)
    }

    pub fn get_tx_count(&self) -> DatabaseResult<u64> {
        self.get(TX_COUNT, Column::Metadata)
            .map(|v| v.unwrap_or_default())
    }
}
