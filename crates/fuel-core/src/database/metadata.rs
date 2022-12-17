use crate::database::{
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_chain_config::ChainConfig;
use fuel_core_types::blockchain::primitives::BlockHeight;

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
pub(crate) const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";
pub(crate) const FINALIZED_DA_HEIGHT_KEY: &[u8] = b"finalized_da_height";
pub(crate) const LAST_PUBLISHED_BLOCK_HEIGHT_KEY: &[u8] = b"last_publish_block_height";

/// Can be used to perform migrations in the future.
pub(crate) const DB_VERSION: u32 = 0;

impl Database {
    pub fn init(&self, config: &ChainConfig) -> DatabaseResult<()> {
        // check only for one field if it initialized or not.
        self.insert(CHAIN_NAME_KEY, Column::Metadata, config.chain_name.clone())
            .and_then(|v: Option<String>| {
                if v.is_some() {
                    Err(DatabaseError::ChainAlreadyInitialized)
                } else {
                    Ok(())
                }
            })?;

        let chain_height = config
            .initial_state
            .as_ref()
            .and_then(|c| c.height)
            .unwrap_or_default();

        let _: Option<u32> = self.insert(DB_VERSION_KEY, Column::Metadata, DB_VERSION)?;
        let _: Option<BlockHeight> =
            self.insert(CHAIN_HEIGHT_KEY, Column::Metadata, chain_height)?;
        Ok(())
    }

    pub fn get_chain_name(&self) -> DatabaseResult<Option<String>> {
        self.get(CHAIN_NAME_KEY, Column::Metadata)
    }

    pub fn get_starting_chain_height(&self) -> DatabaseResult<Option<BlockHeight>> {
        self.get(CHAIN_HEIGHT_KEY, Column::Metadata)
    }
}
