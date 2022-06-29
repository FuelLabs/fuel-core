use crate::config::Config;
use crate::database::columns::METADATA;
use crate::database::Database;
use crate::model::BlockHeight;
use crate::state::Error;

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
pub(crate) const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";
pub(crate) const FINALIZED_DA_HEIGHT_KEY: &[u8] = b"finalized_da_height";
pub(crate) const VALIDATORS_DA_HEIGHT_KEY: &[u8] = b"current_validator_set";
pub(crate) const LAST_COMMITED_FINALIZED_BLOCK_HEIGHT_KEY: &[u8] =
    b"last_commited_finalized_block_height";

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

        self.insert(DB_VERSION_KEY, METADATA, DB_VERSION)?;
        self.insert(CHAIN_HEIGHT_KEY, METADATA, 0)?;
        self.insert(FINALIZED_DA_HEIGHT_KEY, METADATA, 0)?;
        self.insert(VALIDATORS_DA_HEIGHT_KEY, METADATA, 0)?;
        self.insert(LAST_COMMITED_FINALIZED_BLOCK_HEIGHT_KEY, METADATA, 0)?;
        Ok(())
    }

    pub fn get_chain_name(&self) -> Result<Option<String>, Error> {
        self.get(CHAIN_NAME_KEY, METADATA)
    }

    pub fn init_chain_height(&self, height: BlockHeight) -> Result<(), Error> {
        self.insert(CHAIN_HEIGHT_KEY, METADATA, height)
            .and_then(|v| {
                if v.is_some() {
                    Err(Error::ChainAlreadyInitialized)
                } else {
                    Ok(())
                }
            })
    }

    pub fn get_starting_chain_height(&self) -> Result<Option<BlockHeight>, Error> {
        self.get(CHAIN_HEIGHT_KEY, METADATA)
    }
}
