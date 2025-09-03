use crate::{
    blocks::Block,
    result::Result,
};
use fuel_core_services::stream::BoxStream;
use std::fmt;
use tokio::sync::oneshot::{
    Receiver,
    Sender,
    channel,
};

pub trait BlockAggregatorApi: Send + Sync {
    fn await_query(
        &mut self,
    ) -> impl Future<Output = Result<BlockAggregatorQuery>> + Send;
}

pub enum BlockAggregatorQuery {
    GetBlockRange {
        first: u64,
        last: u64,
        response: Sender<BoxStream<Block>>,
    },
    GetCurrentHeight {
        response: Sender<u64>,
    },
}

impl fmt::Debug for BlockAggregatorQuery {
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

impl BlockAggregatorQuery {
    pub fn get_block_range(first: u64, last: u64) -> (Self, Receiver<BoxStream<Block>>) {
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
