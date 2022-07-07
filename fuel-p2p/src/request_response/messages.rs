use std::sync::Arc;

use fuel_core_interfaces::model::{BlockHeight, SealedFuelBlock};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &[u8] = b"/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
/// Currently the only and the biggest message is RequestBlock(BlockHeight)
pub(crate) const MAX_REQUEST_SIZE: usize = 8;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub enum RequestMessage {
    RequestBlock(BlockHeight),
}

/// Final Response Message that p2p service sends to the Orchestrator
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ResponseMessage {
    ResponseBlock(SealedFuelBlock),
}

/// Holds oneshot channels for specific responses
#[derive(Debug)]
pub enum ResponseChannelItem {
    ResponseBlock(oneshot::Sender<SealedFuelBlock>),
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
    ResponseBlock(Arc<SealedFuelBlock>),
}

#[derive(Debug)]
pub enum RequestError {
    NoPeersConnected,
}

#[derive(Debug, PartialEq)]
pub enum ResponseError {
    ResponseChannelDoesNotExist,
    SendingResponseFailed,
    ConversionToIntermediateFailed,
}
