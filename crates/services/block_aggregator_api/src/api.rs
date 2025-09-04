use crate::result::Result;
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
        first: u64,
        last: u64,
        response: Sender<BlockRange>,
    },
    GetCurrentHeight {
        response: Sender<u64>,
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
    pub fn get_block_range(first: u64, last: u64) -> (Self, Receiver<T>) {
        let (sender, receiver) = channel();
        let query = Self::GetBlockRange {
            first,
            last,
            response: sender,
        };
        (query, receiver)
    }

    pub fn get_current_height() -> (Self, Receiver<u64>) {
        let (sender, receiver) = channel();
        let query = Self::GetCurrentHeight { response: sender };
        (query, receiver)
    }
}
