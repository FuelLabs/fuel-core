use fuel_core_types::{
    blockchain::SealedBlockHeader,
    services::p2p::Transactions,
};
use libp2p::PeerId;
use serde::{
    Deserialize,
    Serialize,
};
use std::ops::Range;
use thiserror::Error;
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
#[cfg(test)]
pub(crate) const MAX_REQUEST_SIZE: usize = core::mem::size_of::<RequestMessage>();

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum RequestMessage {
    SealedHeaders(Range<u32>),
    Transactions(Range<u32>),
}

/// Holds oneshot channels for specific responses
#[derive(Debug)]
pub enum ResponseChannelItem {
    SealedHeaders(oneshot::Sender<(PeerId, Option<Vec<SealedBlockHeader>>)>),
    Transactions(oneshot::Sender<Option<Vec<Transactions>>>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseMessage {
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Vec<Transactions>>),
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("Not currently connected to any peers")]
    NoPeersConnected,
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
