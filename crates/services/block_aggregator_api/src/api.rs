use crate::{
    blocks::Block,
    result::{
        Error,
        Result,
    },
};
use fuel_core_services::stream::BoxStream;
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
}
