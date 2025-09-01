use crate::result::{
    Error,
    Result,
};

pub trait BlockSource: Send + Sync {
    fn next_block(&mut self) -> impl Future<Output = Result<Block>>;
}

#[derive(Clone, Copy)]
pub struct Block;
