use crate::protobuf_types::Block as ProtoBlock;
use fuel_core_services::stream::Stream;
use fuel_core_types::fuel_types::BlockHeight;

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<(BlockHeight, ProtoBlock)>),
    /// A remote URL where the blocks can be fetched
    S3(BoxStream<(BlockHeight, RemoteS3Response)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteS3Response {
    pub bucket: String,
    pub key: String,
    pub requester_pays: bool,
    pub aws_endpoint: Option<String>,
}

#[cfg(test)]
impl std::fmt::Debug for BlockRangeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockRangeResponse::Literal(_) => f.debug_struct("Literal").finish(),
            BlockRangeResponse::S3(_url) => f.debug_struct("Remote").finish(),
        }
    }
}
