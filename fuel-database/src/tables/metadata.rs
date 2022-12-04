use crate::{
    Column,
    Database,
    Error,
};
use fuel_core_interfaces::model::BlockHeight;

pub(crate) const DB_VERSION_KEY: &[u8] = b"version";
pub(crate) const CHAIN_NAME_KEY: &[u8] = b"chain_name";
pub(crate) const CHAIN_HEIGHT_KEY: &[u8] = b"chain_height";

/// Can be used to perform migrations in the future.
pub(crate) const DB_VERSION: u32 = 0;

impl Database {
    pub fn init(
        &self,
        chain_height: BlockHeight,
        chain_name: &String,
    ) -> Result<(), Error> {
        // check only for one field if it initialized or not.
        self.insert(CHAIN_NAME_KEY, Column::Metadata, chain_name)
            .and_then(|v: Option<String>| {
                if v.is_some() {
                    Err(Error::ChainAlreadyInitialized)
                } else {
                    Ok(())
                }
            })?;

        let _: Option<u32> = self.insert(DB_VERSION_KEY, Column::Metadata, DB_VERSION)?;
        let _: Option<BlockHeight> =
            self.insert(CHAIN_HEIGHT_KEY, Column::Metadata, chain_height)?;
        Ok(())
    }

    pub fn get_chain_name(&self) -> Result<Option<String>, Error> {
        self.get(CHAIN_NAME_KEY, Column::Metadata)
    }

    pub fn get_starting_chain_height(&self) -> Result<Option<BlockHeight>, Error> {
        self.get(CHAIN_HEIGHT_KEY, Column::Metadata)
    }
}
