use libp2p::request_response::OutboundFailure;
use serde::{Deserialize, Serialize};

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &[u8] = b"/fuel/req_res/0.0.1";
/// Max Size in Bytes of the messages
// todo: this will be defined once Request & Response Messages are clearly defined
// it should be the biggest field of respective enum
pub(crate) const MAX_REQUEST_SIZE: usize = 100;
pub(crate) const MAX_RESPONSE_SIZE: usize = 100;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum RequestMessage {
    RequestBlock,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ResponseMessage {
    ResponseBlock,
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
