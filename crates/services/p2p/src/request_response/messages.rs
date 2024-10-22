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

pub(crate) const REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.1";
pub(crate) const REQUEST_RESPONSE_WITH_ERROR_CODES_PROTOCOL_ID: &str =
    "/fuel/req_res/0.0.2";

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

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ResponseMessageErrorCode {
    /// The peer sent an empty response using protocol `/fuel/req_res/0.0.1`
    #[error("Empty response sent by peer using legacy protocol /fuel/req_res/0.0.1")]
    ProtocolV1EmptyResponse = 0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum V1ResponseMessage {
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Vec<Transactions>>),
    TxPoolAllTransactionsIds(Option<Vec<TxId>>),
    TxPoolFullTransactions(Option<Vec<Option<NetworkableTransactionPool>>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseMessage {
    SealedHeaders(Result<Vec<SealedBlockHeader>, ResponseMessageErrorCode>),
    Transactions(Result<Vec<Transactions>, ResponseMessageErrorCode>),
    TxPoolAllTransactionsIds(Result<Vec<TxId>, ResponseMessageErrorCode>),
    TxPoolFullTransactions(
        Result<Vec<Option<NetworkableTransactionPool>>, ResponseMessageErrorCode>,
    ),
}

impl From<V1ResponseMessage> for ResponseMessage {
    fn from(v1_response: V1ResponseMessage) -> Self {
        match v1_response {
            V1ResponseMessage::SealedHeaders(sealed_headers) => {
                ResponseMessage::SealedHeaders(
                    sealed_headers
                        .ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
            V1ResponseMessage::Transactions(vec) => ResponseMessage::Transactions(
                vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
            ),
            V1ResponseMessage::TxPoolAllTransactionsIds(vec) => {
                ResponseMessage::TxPoolAllTransactionsIds(
                    vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
            V1ResponseMessage::TxPoolFullTransactions(vec) => {
                ResponseMessage::TxPoolFullTransactions(
                    vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
        }
    }
}

impl From<ResponseMessage> for V1ResponseMessage {
    fn from(response: ResponseMessage) -> Self {
        match response {
            ResponseMessage::SealedHeaders(sealed_headers) => {
                V1ResponseMessage::SealedHeaders(sealed_headers.ok())
            }
            ResponseMessage::Transactions(transactions) => {
                V1ResponseMessage::Transactions(transactions.ok())
            }
            ResponseMessage::TxPoolAllTransactionsIds(tx_ids) => {
                V1ResponseMessage::TxPoolAllTransactionsIds(tx_ids.ok())
            }
            ResponseMessage::TxPoolFullTransactions(tx_pool) => {
                V1ResponseMessage::TxPoolFullTransactions(tx_pool.ok())
            }
        }
    }
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
