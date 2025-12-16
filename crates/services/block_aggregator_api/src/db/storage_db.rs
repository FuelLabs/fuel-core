use crate::{
    block_range_response::BlockRangeResponse,
    db::{
        BlocksProvider,
        BlocksStorage,
        table::{
            Blocks,
            Column,
            LatestBlock,
            Mode,
        },
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_services::stream::Stream;
use fuel_core_storage::{
    StorageAsMut,
    StorageAsRef,
    kv_store::KeyValueInspect,
    structured_storage::AsStructuredStorage,
    transactional::{
        AtomicView,
        Modifiable,
        WriteTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

#[cfg(test)]
mod tests;

pub struct StorageBlocksProvider<S> {
    storage: S,
}

impl<S> StorageBlocksProvider<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

pub struct StorageDB<S> {
    storage: S,
}

impl<S> StorageDB<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S> BlocksStorage for StorageDB<S>
where
    S: Modifiable + Send + Sync,
    S: KeyValueInspect<Column = Column>,
{
    type Block = Arc<Vec<u8>>;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        height: BlockHeight,
        block: &Self::Block,
    ) -> Result<()> {
        let current_height = self.get_current_height()?;

        if let Some(current_height) = current_height
            && let Some(next_height) = current_height.succ()
            && height != next_height
        {
            return Err(Error::DB(anyhow!(
                "Cannot store block at height {:?}, expected height {:?}",
                height,
                next_height
            )));
        }

        let mut tx = self.storage.write_transaction();
        tx.storage_as_mut::<Blocks>()
            .insert(&height, block)
            .map_err(|e| Error::DB(anyhow!(e)))?;
        tx.storage_as_mut::<LatestBlock>()
            .insert(&(), &Mode::Local(height))
            .map_err(|e| Error::DB(anyhow!(e)))?;
        tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;

        Ok(())
    }
}

impl<S> BlocksProvider for StorageBlocksProvider<S>
where
    S: 'static,
    S: KeyValueInspect<Column = Column>,
    S: AtomicView,
    S::LatestView: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static,
{
    type Block = Arc<Vec<u8>>;
    type BlockRangeResponse = BlockRangeResponse;

    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<BlockRangeResponse> {
        let latest_view = self
            .storage
            .latest_view()
            .map_err(|e| Error::DB(anyhow!(e)))?;
        let stream = StorageStream::new(latest_view, first, last);
        Ok(BlockRangeResponse::Bytes(Box::pin(stream)))
    }

    fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let height = self
            .storage
            .as_structured_storage()
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?
            .map(|b| b.height());

        Ok(height)
    }
}

impl<S> StorageDB<S>
where
    S: KeyValueInspect<Column = Column>,
{
    pub fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let height = self
            .storage
            .as_structured_storage()
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?
            .map(|b| b.height());

        Ok(height)
    }
}
pub struct StorageStream<S> {
    inner: S,
    next: Option<BlockHeight>,
    last: BlockHeight,
}

impl<S> StorageStream<S> {
    pub fn new(inner: S, first: BlockHeight, last: BlockHeight) -> Self {
        Self {
            inner,
            next: Some(first),
            last,
        }
    }
}

impl<S> Stream for StorageStream<S>
where
    S: Unpin,
    S: KeyValueInspect<Column = Column>,
{
    type Item = (BlockHeight, Arc<Vec<u8>>);

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        tracing::debug!(
            "Polling next block from storage stream, next height: {:?}",
            self.next
        );
        let this = self.get_mut();
        if let Some(height) = this.next {
            let storage = this.inner.as_structured_storage();
            let next_block = storage
                .storage_as_ref::<Blocks>()
                .get(&height)
                .map_err(|e| Error::DB(anyhow!(e)));
            match next_block {
                Ok(Some(block)) => {
                    tracing::debug!("Found block at height: {:?}", height);
                    let next = if height < this.last {
                        Some(BlockHeight::new(*height + 1))
                    } else {
                        None
                    };
                    this.next = next;
                    Poll::Ready(Some((height, block.into_owned())))
                }
                Ok(None) => {
                    tracing::debug!("No block at height: {:?}", height);
                    this.next = None;
                    Poll::Ready(None)
                }
                Err(e) => {
                    tracing::error!("Error while reading next block: {:?}", e);
                    this.next = None;
                    Poll::Ready(None)
                }
            }
        } else {
            Poll::Ready(None)
        }
    }
}
