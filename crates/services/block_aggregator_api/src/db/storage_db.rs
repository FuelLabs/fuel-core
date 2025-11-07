use crate::{
    block_range_response::BlockRangeResponse,
    blocks::BlockSourceEvent,
    db::{
        BlockAggregatorDB,
        table::{
            Blocks,
            Column,
            LatestBlock,
        },
    },
    protobuf_types::{
        Block as ProtoBlock,
        Block,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_services::stream::Stream;
use fuel_core_storage::{
    Error as StorageError,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Modifiable,
        ReadTransaction,
        StorageTransaction,
        WriteTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeSet,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

#[cfg(test)]
mod tests;

pub struct StorageDB<S> {
    highest_new_height: Option<BlockHeight>,
    orphaned_new_height: Option<BlockHeight>,
    storage: S,
}

impl<S> StorageDB<S> {
    pub fn new(storage: S) -> Self {
        Self {
            highest_new_height: None,
            orphaned_new_height: None,
            storage,
        }
    }
}

impl<S, T> StorageDB<S>
where
    S: Modifiable + std::fmt::Debug,
    S: KeyValueInspect<Column = Column>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
    S: AtomicView<LatestView = T>,
    T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + std::fmt::Debug,
{
    fn next_height(&self) -> Result<Option<BlockHeight>> {
        let storage = self
            .storage
            .latest_view()
            .map_err(|e| Error::DB(anyhow!(e)))
            .unwrap();
        let binding = storage.read_transaction();
        let latest_height = binding
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?;
        let next_height = latest_height.and_then(|h| h.succ());
        Ok(next_height)
    }
}

impl<S, T> BlockAggregatorDB for StorageDB<S>
where
    S: Modifiable + std::fmt::Debug,
    S: KeyValueInspect<Column = Column>,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>: StorageMutate<Blocks, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
    S: AtomicView<LatestView = T>,
    T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + std::fmt::Debug,
    StorageTransaction<T>: StorageInspect<Blocks, Error = StorageError>,
{
    type Block = ProtoBlock;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        block_event: BlockSourceEvent<Self::Block>,
    ) -> Result<()> {
        let (height, block) = block_event.clone().into_inner();
        let next_height = self.next_height()?;
        let mut tx = self.storage.write_transaction();
        tx.storage_as_mut::<Blocks>()
            .insert(&height, &block)
            .map_err(|e| Error::DB(anyhow!(e)))?;

        match block_event {
            BlockSourceEvent::NewBlock(new_height, _) => {
                tracing::debug!("New block: {:?}", new_height);
                self.highest_new_height = Some(new_height);
                if self.orphaned_new_height.is_none() {
                    self.orphaned_new_height = Some(new_height);
                }
            }
            BlockSourceEvent::OldBlock(height, _) => {
                tracing::debug!("Old block: {:?}", height);
                let latest_height = if height.succ() == self.orphaned_new_height {
                    self.orphaned_new_height = None;
                    self.highest_new_height.clone().unwrap_or(height)
                } else {
                    height
                };
                tx.storage_as_mut::<LatestBlock>()
                    .insert(&(), &latest_height)
                    .map_err(|e| Error::DB(anyhow!(e)))?;
            }
        }
        tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<BlockRangeResponse> {
        let latest_view = self
            .storage
            .latest_view()
            .map_err(|e| Error::DB(anyhow!(e)))?;
        let stream = StorageStream::new(latest_view, first, last);
        Ok(BlockRangeResponse::Literal(Box::pin(stream)))
    }

    async fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let height = self
            .storage
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?;

        Ok(height.map(|b| b.into_owned()))
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
    S: Unpin + ReadTransaction + std::fmt::Debug,
    for<'a> StorageTransaction<&'a S>: StorageInspect<Blocks, Error = StorageError>,
{
    type Item = ProtoBlock;

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
            let tx = this.inner.read_transaction();
            let next_block = tx
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
                    Poll::Ready(Some(block.into_owned()))
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
