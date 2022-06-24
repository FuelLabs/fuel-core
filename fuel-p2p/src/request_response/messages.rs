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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ResponseMessage {
    ResponseBlock(SealedFuelBlock),
}

#[derive(Debug)]
pub enum ResponseChannelItem {
    ResponseBlock(oneshot::Sender<SealedFuelBlock>),
}

#[derive(Debug)]
pub enum RequestError {
    NoPeersConnected,
}

#[derive(Debug, PartialEq)]
pub enum ResponseError {
    ResponseChannelDoesNotExist,
    SendingResponseFailed,
}
