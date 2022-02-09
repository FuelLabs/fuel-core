use std::collections::HashMap;

use crate::database::columns::METADATA;
use crate::database::Database;
use crate::model::fuel_block::BlockHeight;
use crate::state::Error;

const CHAIN_NAME_KEY: &[u8] = b"chain_name";
const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";
const ETH_FINALIZED_BLOCK_NUMBER: &[u8] = b"eth_finalized_block_number";
const ETH_FINALIZED_FUEL_BLOCK_NUMBER: &[u8] = b"eth_finalized_fuel_block_number";
const CURRET_VALIDATOR_SET: &[u8] = b"eth_finalazed_height";

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
    

    /// set newest finalized eth block
    pub fn set_eth_finalized_block(&self, block: u64) -> Result<u64, Error> {
        self.insert(ETH_FINALIZED_BLOCK_NUMBER, METADATA, block)
            .map(Option::unwrap_or_default)
    }

    /// assume it is allways set sa initialization of database.
    pub fn get_eth_finalized_block(&self) -> Result<u64, Error> {
        self.get(ETH_FINALIZED_BLOCK_NUMBER, METADATA)
        .map(Option::unwrap_or_default)
    }

    /// set last finalized fuel block. In usual case this will be
    pub fn set_fuel_finalized_block(&self, block: u64) -> Result<u64, Error> {
        self.insert(ETH_FINALIZED_FUEL_BLOCK_NUMBER, METADATA, block)
            .map(Option::unwrap_or_default)
    }
    /// Assume it is allways set as initialization of database.
    pub fn get_fuel_finalized_block(&self) -> Result<u64,Error> {
        self.get(ETH_FINALIZED_FUEL_BLOCK_NUMBER, METADATA)
        .map(Option::unwrap_or_default)
    }
}
