use crate::protobuf_types::Block as ProtoBlock;
use fuel_core_services::stream::Stream;
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    collections::HashMap,
    sync::Arc,
};

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// The response to a block range query, either as a literal stream of blocks or as a remote URL
pub enum BlockRangeResponse {
    /// A literal stream of blocks
    Literal(BoxStream<(BlockHeight, ProtoBlock)>),
    /// Bytes of blocks
    Bytes(BoxStream<(BlockHeight, Arc<[u8]>)>),
    /// Remote references: either direct S3 bucket/key metadata or HTTP URLs (e.g. CDN) over the same object keys
    Remote(BoxStream<(BlockHeight, RemoteBlockPayload)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteBlockPayload {
    S3(RemoteS3Response),
    Http(RemoteHttpResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteS3Response {
    pub bucket: String,
    pub key: String,
    pub requester_pays: bool,
    pub aws_endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHttpResponse {
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[cfg(test)]
impl std::fmt::Debug for BlockRangeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockRangeResponse::Literal(_) => f.debug_struct("Literal").finish(),
            BlockRangeResponse::Remote(_) => f.debug_struct("Remote").finish(),
            BlockRangeResponse::Bytes(_) => f.debug_struct("Bytes").finish(),
        }
    }
}
