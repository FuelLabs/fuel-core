use crate::database::Database;
use fuel_core_storage::{
    not_found,
    tables::FuelBlocks,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::Bytes32,
};
pub const DEFAULT_NAME: &str = "Fuel.testnet";

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait ChainQueryData {
    fn name(&self) -> StorageResult<String>;
    fn latest_block(&self) -> StorageResult<(BlockHeader, Vec<Bytes32>)>;
}

pub struct ChainQueryContext<'a>(pub &'a Database);

impl ChainQueryData for ChainQueryContext<'_> {
    fn latest_block(
        &self,
    ) -> StorageResult<(
        fuel_core_types::blockchain::header::BlockHeader,
        Vec<fuel_core_types::fuel_types::Bytes32>,
    )> {
        let db = self.0;

        let height = db.get_block_height()?.unwrap_or_default();
        let id = db.get_block_id(height)?.unwrap_or_default();
        let mut block = db
            .storage::<FuelBlocks>()
            .get(&id)?
            .ok_or(not_found!(FuelBlocks))?
            .into_owned();

        let tx_ids: Vec<fuel_core_types::fuel_types::Bytes32> = block
            .transactions_mut()
            .iter()
            .map(|tx| tx.to_owned())
            .collect();

        let header = block.header().clone();

        Ok((header, tx_ids))
    }

    fn name(&self) -> fuel_core_storage::Result<String> {
        let db = self.0;

        let name = db
            .get_chain_name()?
            .unwrap_or_else(|| DEFAULT_NAME.to_string());

        Ok(name)
    }
}
