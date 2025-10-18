use crate::{
    NewBlock,
    result::Result,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::fmt;

/// The API for querying the block aggregator service.
pub trait BlockAggregatorApi: Send + Sync {
    /// The type of the block range response.
    type BlockRangeResponse;

    /// Awaits the next query to the block aggregator service.
    fn await_query(
        &mut self,
    ) -> impl Future<Output = Result<BlockAggregatorQuery<Self::BlockRangeResponse>>> + Send;
}

pub enum BlockAggregatorQuery<BlockRangeResponse> {
    GetBlockRange {
        first: BlockHeight,
        last: BlockHeight,
        response: tokio::sync::oneshot::Sender<BlockRangeResponse>,
    },
    GetCurrentHeight {
        response: tokio::sync::oneshot::Sender<BlockHeight>,
    },
    // TODO: Do we need a way to unsubscribe or can we just see that the receiver is dropped?
    NewBlockSubscription {
        response: tokio::sync::mpsc::Sender<NewBlock>,
    },
}

impl<T> fmt::Debug for BlockAggregatorQuery<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockAggregatorQuery::GetBlockRange { first, last, .. } => f
                .debug_struct("GetBlockRange")
                .field("first", first)
                .field("last", last)
                .finish(),
            BlockAggregatorQuery::GetCurrentHeight { .. } => {
                f.debug_struct("GetCurrentHeight").finish()
            }
            BlockAggregatorQuery::NewBlockSubscription { .. } => {
                f.debug_struct("GetNewBlockStream").finish()
            }
        }
    }
}

#[cfg(test)]
impl<T> BlockAggregatorQuery<T> {
    pub fn get_block_range<H: Into<BlockHeight>>(
        first: H,
        last: H,
    ) -> (Self, tokio::sync::oneshot::Receiver<T>) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let first: BlockHeight = first.into();
        let last: BlockHeight = last.into();
        let query = Self::GetBlockRange {
            first,
            last,
            response: sender,
        };
        (query, receiver)
    }

    pub fn get_current_height() -> (Self, tokio::sync::oneshot::Receiver<BlockHeight>) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let query = Self::GetCurrentHeight { response: sender };
        (query, receiver)
    }

    pub fn new_block_subscription() -> (Self, tokio::sync::mpsc::Receiver<NewBlock>) {
        const ARBITRARY_CHANNEL_SIZE: usize = 10;
        let (sender, receiver) = tokio::sync::mpsc::channel(ARBITRARY_CHANNEL_SIZE);
        let query = Self::NewBlockSubscription { response: sender };
        (query, receiver)
    }
}
