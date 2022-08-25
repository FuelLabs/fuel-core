use crate::{
    database::{
        columns::METADATA,
        Database,
    },
    model::BlockHeight,
    service::config::Config,
    state::Error,
};

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
pub(crate) const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";
pub(crate) const FINALIZED_DA_HEIGHT_KEY: &[u8] = b"finalized_da_height";
pub(crate) const VALIDATORS_DA_HEIGHT_KEY: &[u8] = b"current_validator_set";
pub(crate) const LAST_COMMITTED_FINALIZED_BLOCK_HEIGHT_KEY: &[u8] =
    b"last_committed_finalized_block_height";

/// Can be used to perform migrations in the future.
pub(crate) const DB_VERSION: u32 = 0;

impl Database {
    pub fn init(&self, config: &Config) -> Result<(), Error> {
        // check only for one field if it initialized or not.
        self.insert(
            CHAIN_NAME_KEY,
            METADATA,
            config.chain_conf.chain_name.clone(),
        )
        .and_then(|v| {
            if v.is_some() {
                Err(Error::ChainAlreadyInitialized)
            } else {
                Ok(())
            }
        })?;

        let chain_height = config
            .chain_conf
            .initial_state
            .as_ref()
            .and_then(|c| c.height)
            .unwrap_or_default();

        self.insert(DB_VERSION_KEY, METADATA, DB_VERSION)?;
        self.insert(CHAIN_HEIGHT_KEY, METADATA, chain_height)?;
        self.insert(FINALIZED_DA_HEIGHT_KEY, METADATA, 0)?;
        self.insert(VALIDATORS_DA_HEIGHT_KEY, METADATA, 0)?;
        self.insert(LAST_COMMITTED_FINALIZED_BLOCK_HEIGHT_KEY, METADATA, 0)?;
        Ok(())
    }

    pub fn get_chain_name(&self) -> Result<Option<String>, Error> {
        self.get(CHAIN_NAME_KEY, METADATA)
    }

    pub fn get_starting_chain_height(&self) -> Result<Option<BlockHeight>, Error> {
        self.get(CHAIN_HEIGHT_KEY, METADATA)
    }
}
