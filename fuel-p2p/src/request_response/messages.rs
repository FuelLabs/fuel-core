use libp2p::request_response::OutboundFailure;
use serde::{Deserialize, Serialize};

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
