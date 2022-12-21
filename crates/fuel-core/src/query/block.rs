use crate::database::Database;
use fuel_core_storage::{
    tables::FuelBlocks,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::Bytes32,
};

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait BlockQueryData {
    fn block(
        &self,
        id: Bytes32,
    ) -> std::result::Result<StorageResult<(BlockHeader, Vec<Bytes32>)>, anyhow::Error>;

    fn block_id(&self, height: u64) -> std::result::Result<Bytes32, anyhow::Error>;
}

pub struct BlockQueryContext<'a>(pub &'a Database);

impl BlockQueryData for BlockQueryContext<'_> {
    fn block(
        &self,
        id: fuel_core_types::fuel_types::Bytes32,
    ) -> std::result::Result<
        StorageResult<(BlockHeader, Vec<fuel_core_types::fuel_types::Bytes32>)>,
        anyhow::Error,
    > {
        let db = self.0;

        let mut block = db
            .storage::<FuelBlocks>()
            .get(&id)?
            .ok_or_else(|| fuel_core_storage::Error::NotFound("Block Not Found", ""))?
            .into_owned();

        let tx_ids: Vec<fuel_core_types::fuel_types::Bytes32> = block
            .transactions_mut()
            .iter()
            .map(|tx| tx.to_owned())
            .collect();

        let header = block.header().clone();

        Ok(Ok((header, tx_ids)))
    }

    fn block_id(
        &self,
        height: u64,
    ) -> std::result::Result<fuel_core_types::fuel_types::Bytes32, anyhow::Error> {
        let db = self.0;
        let id = db
            .get_block_id(height.try_into()?)?
            .ok_or_else(|| fuel_core_storage::Error::NotFound("Block Not Found", ""))?;

        Ok(id)
    }
}
