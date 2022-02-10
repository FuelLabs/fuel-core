use crate::database::columns::METADATA;
use crate::database::Database;
use crate::model::fuel_block::BlockHeight;
use crate::state::Error;

pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
pub(crate) const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";
pub(crate) const ETH_FINALIZED_BLOCK_NUMBER: &[u8] = b"eth_finalized_block_number";
pub(crate) const ETH_FINALIZED_FUEL_BLOCK_NUMBER: &[u8] = b"eth_finalized_fuel_block_number";
pub(crate) const CURRET_VALIDATOR_SET_BLOCK: &[u8] = b"current_validator_set";

impl Database {
    pub fn init_chain_name(&self, name: String) -> Result<(), Error> {
        self.insert(CHAIN_NAME_KEY, METADATA, name).and_then(|v| {
            if v.is_some() {
                Err(Error::ChainAlreadyInitialized)
            } else {
                Ok(())
            }
        })
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
