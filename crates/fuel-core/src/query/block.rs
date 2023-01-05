use crate::{
    database::Database,
    state::IterDirection,
};
use fuel_core_storage::{
    tables::FuelBlocks,
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock as Block,
        header::BlockHeader,
        primitives::BlockHeight,
    },
    fuel_types::Bytes32,
};

/// Trait that specifies all the data required by the output message query.
pub trait BlockQueryData {
    type Item<'a>
    where
        Self: 'a;
    type Iter<'a>: Iterator<Item = Self::Item<'a>>
    where
        Self: 'a;

    fn block(
        &self,
        id: Bytes32,
    ) -> std::result::Result<StorageResult<(BlockHeader, Vec<Bytes32>)>, anyhow::Error>;

    fn block_id(&self, height: u64) -> std::result::Result<Bytes32, anyhow::Error>;

    fn blocks(
        &self,
        start: Option<usize>,
        direction: IterDirection,
    ) -> StorageResult<Self::Iter<'_>>;
}

pub struct BlockQueryContext<'a>(pub &'a Database);

impl BlockQueryData for BlockQueryContext<'_> {
    type Item<'a> = StorageResult<(BlockHeight, Block)> where Self: 'a;
    type Iter<'a> = Box< dyn Iterator<Item = StorageResult<(BlockHeight, Block)>> + 'a> where Self: 'a;

    fn block(
        &self,
        id: fuel_core_types::fuel_types::Bytes32,
    ) -> std::result::Result<
        StorageResult<(BlockHeader, Vec<fuel_core_types::fuel_types::Bytes32>)>,
        anyhow::Error,
    > {
        let db = self.0;

        let block = db
            .storage::<FuelBlocks>()
            .get(&id)?
            .ok_or_else(|| fuel_core_storage::Error::NotFound("Block Not Found", ""))?
            .into_owned();

        let tx_ids: Vec<fuel_core_types::fuel_types::Bytes32> = block
            .transactions()
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

    fn blocks(
        &self,
        start: Option<usize>,
        direction: IterDirection,
    ) -> StorageResult<Self::Iter<'_>> {
        let db = self.0;

        let blocks = db.all_block_ids(start.map(Into::into), Some(direction));

        let blocks = blocks.into_iter().map(|result| {
            result.map_err(StorageError::from).and_then(|(height, id)| {
                let block = db
                    .storage::<FuelBlocks>()
                    .get(&id)
                    .transpose()
                    .ok_or(fuel_core_storage::not_found!(FuelBlocks))??
                    .into_owned();

                Ok((height, block))
            })
        });

        Ok(Box::new(blocks))
    }
}
