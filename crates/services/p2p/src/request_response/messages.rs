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

use crate::service::TaskError;

pub(crate) const V1_REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.1";
pub(crate) const V2_REQUEST_RESPONSE_PROTOCOL_ID: &str = "/fuel/req_res/0.0.2";

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
    #[error("The requested range is too large")]
    RequestedRangeTooLarge = 1,
    #[error("Timeout while processing request")]
    Timeout = 2,
    #[error("Sync processor is out of capacity")]
    SyncProcessorOutOfCapacity = 3,
    #[error("The peer sent an unknown error code")]
    #[serde(skip_serializing, other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum V1ResponseMessage {
    SealedHeaders(Option<Vec<SealedBlockHeader>>),
    Transactions(Option<Vec<Transactions>>),
    TxPoolAllTransactionsIds(Option<Vec<TxId>>),
    TxPoolFullTransactions(Option<Vec<Option<NetworkableTransactionPool>>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum V2ResponseMessage {
    SealedHeaders(Result<Vec<SealedBlockHeader>, ResponseMessageErrorCode>),
    Transactions(Result<Vec<Transactions>, ResponseMessageErrorCode>),
    TxPoolAllTransactionsIds(Result<Vec<TxId>, ResponseMessageErrorCode>),
    TxPoolFullTransactions(
        Result<Vec<Option<NetworkableTransactionPool>>, ResponseMessageErrorCode>,
    ),
}

impl From<V1ResponseMessage> for V2ResponseMessage {
    fn from(v1_response: V1ResponseMessage) -> Self {
        match v1_response {
            V1ResponseMessage::SealedHeaders(sealed_headers) => {
                V2ResponseMessage::SealedHeaders(
                    sealed_headers
                        .ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
            V1ResponseMessage::Transactions(vec) => V2ResponseMessage::Transactions(
                vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
            ),
            V1ResponseMessage::TxPoolAllTransactionsIds(vec) => {
                V2ResponseMessage::TxPoolAllTransactionsIds(
                    vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
            V1ResponseMessage::TxPoolFullTransactions(vec) => {
                V2ResponseMessage::TxPoolFullTransactions(
                    vec.ok_or(ResponseMessageErrorCode::ProtocolV1EmptyResponse),
                )
            }
        }
    }
}

impl From<V2ResponseMessage> for V1ResponseMessage {
    fn from(response: V2ResponseMessage) -> Self {
        match response {
            V2ResponseMessage::SealedHeaders(sealed_headers) => {
                V1ResponseMessage::SealedHeaders(sealed_headers.ok())
            }
            V2ResponseMessage::Transactions(transactions) => {
                V1ResponseMessage::Transactions(transactions.ok())
            }
            V2ResponseMessage::TxPoolAllTransactionsIds(tx_ids) => {
                V1ResponseMessage::TxPoolAllTransactionsIds(tx_ids.ok())
            }
            V2ResponseMessage::TxPoolFullTransactions(tx_pool) => {
                V1ResponseMessage::TxPoolFullTransactions(tx_pool.ok())
            }
        }
    }
}

pub type OnResponse<T> = oneshot::Sender<(PeerId, Result<T, ResponseError>)>;
// This type is more complex because it's used in tasks that need to select a peer to send the request and this
// can cause errors where the peer is not defined.
pub type OnResponseWithPeerSelection<T> =
    oneshot::Sender<Result<(PeerId, Result<T, ResponseError>), TaskError>>;

#[derive(Debug)]
pub enum ResponseSender {
    SealedHeaders(
        OnResponseWithPeerSelection<
            Result<Vec<SealedBlockHeader>, ResponseMessageErrorCode>,
        >,
    ),
    Transactions(
        OnResponseWithPeerSelection<Result<Vec<Transactions>, ResponseMessageErrorCode>>,
    ),
    TransactionsFromPeer(OnResponse<Result<Vec<Transactions>, ResponseMessageErrorCode>>),

    TxPoolAllTransactionsIds(OnResponse<Result<Vec<TxId>, ResponseMessageErrorCode>>),
    TxPoolFullTransactions(
        OnResponse<
            Result<Vec<Option<NetworkableTransactionPool>>, ResponseMessageErrorCode>,
        >,
    ),
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

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::ResponseMessageErrorCode;

    #[test]
    fn response_message_error_code__unknown_error_cannot_be_serialized() {
        let error = super::ResponseMessageErrorCode::Unknown;
        let serialized = postcard::to_allocvec(&error);
        assert!(serialized.is_err());
    }

    #[test]
    fn response_message_error_code__known_error_code_is_deserialized_to_variant() {
        let serialized_error_code =
            postcard::to_stdvec(&ResponseMessageErrorCode::ProtocolV1EmptyResponse)
                .unwrap();
        println!("Error code: {:?}", serialized_error_code);
        let response_message_error_code: ResponseMessageErrorCode =
            postcard::from_bytes(&serialized_error_code).unwrap();
        assert!(matches!(
            response_message_error_code,
            ResponseMessageErrorCode::ProtocolV1EmptyResponse
        ));
    }

    #[test]
    fn response_message_error_code__unknown_error_code_is_deserialized_to_unknown_variant(
    ) {
        let serialized_error_code = vec![42];
        let response_message_error_code: ResponseMessageErrorCode =
            postcard::from_bytes(&serialized_error_code).unwrap();
        assert!(matches!(
            response_message_error_code,
            ResponseMessageErrorCode::Unknown
        ));
    }
}
