use crate::{
    blocks::Block,
    result::{
        Error,
        Result,
    },
};
use tokio::sync::mpsc::{
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
        response: Sender<Result<Block>>,
    },
}

impl BlockAggregatorQuery {
    pub fn get_block_range(first: u64, last: u64) -> (Self, Receiver<Result<Block>>) {
        let (sender, receiver) = channel(100);
        let query = Self::GetBlockRange {
            first,
            last,
            response: sender,
        };
        (query, receiver)
    }
}
