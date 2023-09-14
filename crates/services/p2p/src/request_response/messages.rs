use std::{
    ops::Range,
    sync::Arc,
};

use fuel_core_types::{
    blockchain::{
        primitives::BlockId,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};
use libp2p::PeerId;
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    FromInto,
};
use thiserror::Error;
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &[u8] = b"/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
pub(crate) const MAX_REQUEST_SIZE: usize = core::mem::size_of::<RequestMessage>();

// Peer receives a `RequestMessage`.
// It prepares a response in form of `OutboundResponse`
// This `OutboundResponse` gets prepared to be sent over the wire in `NetworkResponse` format.
// The Peer that requested the message receives the response over the wire in `NetworkResponse` format.
// It then unpacks it into `ResponseMessage`.
// `ResponseChannelItem` is used to forward the data within `ResponseMessage` to the receving channel.
// Client Peer: `RequestMessage` (send request)
// Server Peer: `RequestMessage` (receive request) -> `OutboundResponse` -> `NetworkResponse` (send response)
// Client Peer: `NetworkResponse` (receive response) -> `ResponseMessage(data)` -> `ResponseChannelItem(channel, data)` (handle response)

#[serde_as]
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum RequestMessage {
    Block(BlockHeight),
    SealedHeaders(Range<u32>),
    Transactions(#[serde_as(as = "FromInto<[u8; 32]>")] BlockId),
}

/// Final Response Message that p2p service sends to the Orchestrator
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ResponseMessage {
    SealedBlock(Box<Option<SealedBlock>>),
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Vec<Transaction>>),
}

/// Holds oneshot channels for specific responses
#[derive(Debug)]
pub enum ResponseChannelItem {
    Block(oneshot::Sender<Option<SealedBlock>>),
    SealedHeaders(oneshot::Sender<(PeerId, Option<Vec<SealedBlockHeader>>)>),
    Transactions(oneshot::Sender<Option<Vec<Transaction>>>),
}

/// Response that is sent over the wire
/// and then additionaly deserialized into `ResponseMessage`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NetworkResponse {
    Block(Option<Vec<u8>>),
    Headers(Option<Vec<u8>>),
    Transactions(Option<Vec<u8>>),
}

/// Initial state of the `ResponseMessage` prior to having its inner value serialized
/// and wrapped into `NetworkResponse`
#[derive(Debug, Clone)]
pub enum OutboundResponse {
    Block(Option<Arc<SealedBlock>>),
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Arc<Vec<Transaction>>>),
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("Not currently connected to any peers")]
    NoPeersConnected,
}

#[derive(Debug, Eq, PartialEq, Error)]
pub enum ResponseError {
    #[error("Response channel does not exist")]
    ResponseChannelDoesNotExist,
    #[error("Failed to send response")]
    SendingResponseFailed,
    #[error("Failed to convert response to intermediate format")]
    ConversionToIntermediateFailed,
}
