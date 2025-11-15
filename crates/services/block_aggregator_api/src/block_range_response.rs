use crate::protobuf_types::Block as ProtoBlock;
use fuel_core_services::stream::Stream;

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<ProtoBlock>),
    /// A remote URL where the blocks can be fetched
    Remote(BoxStream<RemoteBlockRangeResponse>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBlockRangeResponse {
    pub region: String,
    pub bucket: String,
    pub key: String,
    pub url: String,
}

#[cfg(test)]
impl std::fmt::Debug for BlockRangeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockRangeResponse::Literal(_) => f.debug_struct("Literal").finish(),
            BlockRangeResponse::Remote(_url) => f.debug_struct("Remote").finish(),
        }
    }
}
