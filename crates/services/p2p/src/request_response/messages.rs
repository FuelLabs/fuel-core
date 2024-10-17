use fuel_core_types::{
    blockchain::SealedBlockHeader,
    fuel_tx::TxId,
    services::p2p::{
        NetworkableTransactionPool,
        Transactions,
    },
};
use libp2p::{
    request_response::OutboundFailure,
    PeerId,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::ops::Range;
use thiserror::Error;
use tokio::sync::oneshot;

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID_V1: &str = "/fuel/req_res/0.0.1";

/// Max Size in Bytes of the Request Message
#[cfg(test)]
pub(crate) const MAX_REQUEST_SIZE: usize = core::mem::size_of::<RequestMessage>();

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum RequestMessage {
    SealedHeaders(Range<u32>),
    Transactions(Range<u32>),
    TxPoolAllTransactionsIds,
    TxPoolFullTransactions(Vec<TxId>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseMessage {
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Vec<Transactions>>),
    TxPoolAllTransactionsIds(Option<Vec<TxId>>),
    TxPoolFullTransactions(Option<Vec<Option<NetworkableTransactionPool>>>),
}

pub type OnResponse<T> = oneshot::Sender<(PeerId, Result<T, ResponseError>)>;

#[derive(Debug)]
pub enum ResponseSender {
    SealedHeaders(OnResponse<Option<Vec<SealedBlockHeader>>>),
    Transactions(OnResponse<Option<Vec<Transactions>>>),
    TxPoolAllTransactionsIds(OnResponse<Option<Vec<TxId>>>),
    TxPoolFullTransactions(OnResponse<Option<Vec<Option<NetworkableTransactionPool>>>>),
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("Not currently connected to any peers")]
    NoPeersConnected,
}

#[derive(Debug, Error)]
pub enum ResponseError {
    /// This is the raw error from [`libp2p-request-response`]
    #[error("P2P outbound error {0}")]
    P2P(OutboundFailure),
    /// The peer responded with an invalid response type
    #[error("Peer response message was of incorrect type")]
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
