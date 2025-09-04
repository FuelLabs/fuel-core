use crate::result::Result;
use fuel_core_types::fuel_types::BlockHeight;
use std::fmt;
use tokio::sync::oneshot::{
    Receiver,
    Sender,
    channel,
};

/// The API for querying the block aggregator service.
pub trait BlockAggregatorApi: Send + Sync {
    /// The type of the block range response.
    type BlockRangeResponse;

    /// Awaits the next query to the block aggregator service.
    fn await_query(
        &mut self,
    ) -> impl Future<Output = Result<BlockAggregatorQuery<Self::BlockRangeResponse>>> + Send;
}

pub enum BlockAggregatorQuery<BlockRange> {
    GetBlockRange {
        first: BlockHeight,
        last: BlockHeight,
        response: Sender<BlockRange>,
    },
    GetCurrentHeight {
        response: Sender<BlockHeight>,
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
        }
    }
}

impl<T> BlockAggregatorQuery<T> {
    pub fn get_block_range<H: Into<BlockHeight>>(
        first: H,
        last: H,
    ) -> (Self, Receiver<T>) {
        let (sender, receiver) = channel();
        let first: BlockHeight = first.into();
        let last: BlockHeight = last.into();
        let query = Self::GetBlockRange {
            first,
            last,
            response: sender,
        };
        (query, receiver)
    }

    pub fn get_current_height() -> (Self, Receiver<BlockHeight>) {
        let (sender, receiver) = channel();
        let query = Self::GetCurrentHeight { response: sender };
        (query, receiver)
    }
}
