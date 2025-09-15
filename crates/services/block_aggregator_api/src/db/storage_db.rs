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
    cmp::Ordering,
    collections::BTreeSet,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};
use table::Blocks;

pub mod table;
#[cfg(test)]
mod tests;

pub struct StorageDB<S> {
    highest_contiguous_block: BlockHeight,
    orphaned_heights: BTreeSet<BlockHeight>,
    storage: S,
}

impl<S> StorageDB<S> {
    pub fn new(storage: S) -> Self {
        let height = BlockHeight::new(0);
        Self::new_with_height(storage, height)
    }

    pub fn new_with_height(storage: S, highest_contiguous_block: BlockHeight) -> Self {
        let orphaned_heights = BTreeSet::new();
        Self {
            highest_contiguous_block,
            orphaned_heights,
            storage,
        }
    }

    fn update_highest_contiguous_block(&mut self, height: BlockHeight) {
        let next_height = self.next_height();
        match height.cmp(&next_height) {
            Ordering::Equal => {
                self.highest_contiguous_block = height;
                while let Some(next_height) = self.orphaned_heights.first() {
                    if next_height == &self.next_height() {
                        self.highest_contiguous_block = *next_height;
                        let _ = self.orphaned_heights.pop_first();
                    } else {
                        break;
                    }
                }
            }
            Ordering::Greater => {
                self.orphaned_heights.insert(height);
            }
            Ordering::Less => {
                tracing::warn!(
                    "Received block at height {:?}, but the syncing is already at height {:?}. Ignoring block.",
                    height,
                    self.highest_contiguous_block
                );
            }
        }
    }
    fn next_height(&self) -> BlockHeight {
        let last_height = *self.highest_contiguous_block;
        BlockHeight::new(last_height.saturating_add(1))
    }
}

impl<S, T> BlockAggregatorDB for StorageDB<S>
where
    S: Modifiable + std::fmt::Debug,
    S: KeyValueInspect<Column = Column>,
    for<'b> StorageTransaction<&'b mut S>: StorageMutate<Blocks, Error = StorageError>,
    S: AtomicView<LatestView = T>,
    T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + std::fmt::Debug,
    StorageTransaction<T>: AtomicView + StorageInspect<Blocks, Error = StorageError>,
{
    type BlockRange = BlockRangeResponse;

    async fn store_block(&mut self, height: BlockHeight, block: Block) -> Result<()> {
        self.update_highest_contiguous_block(height);
        let mut tx = self.storage.write_transaction();
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
    ) -> Result<BlockRangeResponse> {
        let latest_view = self
            .storage
            .latest_view()
            .map_err(|e| Error::DB(anyhow!(e)))?;
        let stream = StorageStream::new(latest_view, first, last);
        Ok(BlockRangeResponse::Literal(Box::pin(stream)))
    }

    async fn get_current_height(&self) -> Result<BlockHeight> {
        Ok(self.highest_contiguous_block)
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
    type Item = Block;

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
