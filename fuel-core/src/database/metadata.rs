use crate::database::columns::METADATA;
use crate::database::Database;
use crate::model::BlockHeight;
use crate::state::Error;

const CHAIN_NAME_KEY: &[u8] = b"chain_name";
const START_HEIGHT_KEY: &[u8] = b"chain_height";

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
        self.insert(START_HEIGHT_KEY, METADATA, height)
            .and_then(|v| {
                if v.is_some() {
                    Err(Error::ChainAlreadyInitialized)
                } else {
                    Ok(())
                }
            })
    }

    pub fn get_starting_chain_height(&self) -> Result<Option<BlockHeight>, Error> {
        self.get(START_HEIGHT_KEY, METADATA)
    }
}
