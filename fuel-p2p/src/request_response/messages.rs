use fuel_core_interfaces::model::{BlockHeight, FuelBlock};
use libp2p::request_response::OutboundFailure;
use serde::{Deserialize, Serialize};

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
    ResponseBlock(FuelBlock),
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

#[derive(Debug, PartialEq)]
pub enum ReqResNetworkError {
    DialFailure,
    Timeout,
    ConnectionClosed,
    UnsupportedProtocols,
}

impl From<OutboundFailure> for ReqResNetworkError {
    fn from(err: OutboundFailure) -> Self {
        match err {
            OutboundFailure::DialFailure => Self::DialFailure,
            OutboundFailure::Timeout => Self::Timeout,
            OutboundFailure::ConnectionClosed => Self::ConnectionClosed,
            OutboundFailure::UnsupportedProtocols => Self::UnsupportedProtocols,
        }
    }
}
