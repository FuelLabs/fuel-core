use crate::{
    blocks::Block,
    result::Result,
};
use fuel_core_services::stream::Stream;

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send>>;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<Result<Block>>),
    /// A remote URL where the blocks can be fetched
    Remote(String),
}
