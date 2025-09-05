use crate::{
    block_range_response::BlockRangeResponse,
    blocks::Block,
    db::{
        BlockAggregatorDB,
        storage_db::table::Column,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_storage::{
    Error as StorageError,
    Mappable,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
    iter::{
        IntoBoxedIter,
        IterDirection,
        IterableStore,
        IterableTable,
        IteratorOverTable,
    },
    kv_store::KeyValueInspect,
    transactional::{
        Modifiable,
        ReadTransaction,
        StorageTransaction,
        WriteTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use table::Blocks;

pub mod table;
#[cfg(test)]
mod tests;

pub struct StorageDB<S> {
    inner: S,
}

impl<S> StorageDB<S> {
    pub fn new(storage: S) -> Self {
        Self { inner: storage }
    }
}

impl<S> BlockAggregatorDB for StorageDB<S>
where
    S: Send + Sync + Modifiable + Clone + 'static,
    S: IterableTable<Blocks>,
    for<'a> StorageTransaction<&'a mut S>: StorageMutate<Blocks, Error = StorageError>,
{
    type BlockRange = BlockRangeResponse;

    async fn store_block(&mut self, height: BlockHeight, block: Block) -> Result<()> {
        let mut tx = self.inner.write_transaction();
        tx.storage_as_mut::<Blocks>()
            .insert(&height, &block)
            .map_err(|e| Error::DB(anyhow!(e)))?;
        tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<Self::BlockRange> {
        let iter = self
            .inner
            .iter_all_by_start::<Blocks>(Some(&first), Some(IterDirection::Forward))
            .take_while(move |res| match res {
                Ok((height, _)) => *height <= last,
                _ => true,
            })
            .map(|res| res.map(|(_, block)| block))
            .map(|res| res.map_err(|e| Error::DB(anyhow!(e))));
        let stream = futures::stream::iter(iter);
        Ok(BlockRangeResponse::Literal(Box::pin(stream)))
    }

    async fn get_current_height(&self) -> Result<BlockHeight> {
        todo!()
    }
}
