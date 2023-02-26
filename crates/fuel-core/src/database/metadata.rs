use crate::database::{
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_chain_config::ChainConfig;

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";

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
}
