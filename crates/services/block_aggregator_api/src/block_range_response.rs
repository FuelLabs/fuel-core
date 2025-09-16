use crate::blocks::Block;
use fuel_core_services::stream::Stream;
use std::fmt;

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<Block>),
    /// A remote URL where the blocks can be fetched
    Remote(String),
}

#[cfg(test)]
impl std::fmt::Debug for BlockRangeResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockRangeResponse::Literal(_) => f.debug_struct("Literal").finish(),
            BlockRangeResponse::Remote(url) => {
                f.debug_struct("Remote").field("url", url).finish()
            }
        }
    }
}
