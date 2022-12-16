use std::sync::Arc;

use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &[u8] = b"/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
/// Currently the only and the biggest message is RequestBlock(BlockHeight)
pub(crate) const MAX_REQUEST_SIZE: usize = 8;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, Copy)]
pub enum RequestMessage {
    RequestBlock(BlockHeight),
}

/// Final Response Message that p2p service sends to the Orchestrator
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ResponseMessage {
    ResponseBlock(SealedBlock),
}

/// Holds oneshot channels for specific responses
#[derive(Debug)]
pub enum ResponseChannelItem {
    ResponseBlock(oneshot::Sender<SealedBlock>),
}

/// Response that is sent over the wire
/// and then additionaly deserialized into `ResponseMessage`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum IntermediateResponse {
    ResponseBlock(Vec<u8>),
}

/// Initial state of the `ResponseMessage` prior to having its inner value serialized
/// and wrapped into `IntermediateResponse`
#[derive(Debug, Clone)]
pub enum OutboundResponse {
    ResponseBlock(Arc<SealedBlock>),
}

#[derive(Debug)]
pub enum RequestError {
    NoPeersConnected,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ResponseError {
    ResponseChannelDoesNotExist,
    SendingResponseFailed,
    ConversionToIntermediateFailed,
}
