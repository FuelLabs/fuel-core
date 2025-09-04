use fuel_core_services::stream::BoxStream;
use crate::blocks::Block;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<Block>),
    /// A remote URL where the blocks can be fetched
    Remote(String),
}