use std::{
    ops::Range,
    sync::Arc,
};

use fuel_core_types::{
    blockchain::{
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::Transactions,
};
use libp2p::PeerId;
use libp2p_request_response::OutboundFailure;
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
#[cfg(test)]
pub(crate) const MAX_REQUEST_SIZE: usize = core::mem::size_of::<RequestMessage>();

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum RequestMessage {
    Block(BlockHeight),
    SealedHeaders(Range<u32>),
    Transactions(Range<u32>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseMessage {
    Block(Option<Arc<SealedBlock>>),
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Arc<Vec<Transactions>>>),
}

pub type OnResponse<T> = oneshot::Sender<Result<T, ResponseError>>;

#[derive(Debug)]
pub enum TypedResponseChannel {
    Block(OnResponse<Option<Arc<SealedBlock>>>),
    SealedHeaders(OnResponse<(PeerId, Option<Vec<SealedBlockHeader>>)>),
    Transactions(OnResponse<Option<Arc<Vec<Transactions>>>>),
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("Not currently connected to any peers")]
    NoPeersConnected,
}

#[derive(Debug)]
pub struct ResponseError {
    /// This is the peer that the request was sent to
    pub peer: PeerId,
    pub kind: ResponseErrorKind,
}

#[derive(Debug)]
pub enum ResponseErrorKind {
    /// This is the raw error from [`libp2p-request-response`]
    P2P(OutboundFailure),
    /// The peer responded with an invalid response type
    TypeMismatch,
}

/// Errors than can occur when attempting to send a response
#[derive(Debug, Eq, PartialEq, Error)]
pub enum ResponseSendError {
    #[error("Response channel does not exist")]
    ResponseChannelDoesNotExist,
    #[error("Failed to send response")]
    SendingResponseFailed,
    #[error("Failed to convert response to intermediate format")]
    ConversionToIntermediateFailed,
}
