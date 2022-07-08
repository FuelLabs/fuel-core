use super::{GossipsubCodec, NetworkCodec, RequestResponseConverter};
use crate::{
    gossipsub::messages::{GossipTopicTag, GossipsubBroadcastRequest, GossipsubMessage},
    request_response::messages::{
        IntermediateResponse, OutboundResponse, RequestMessage, ResponseMessage, MAX_REQUEST_SIZE,
        REQUEST_RESPONSE_PROTOCOL_ID,
    },
};
use async_trait::async_trait;
use futures::{AsyncRead, AsyncWriteExt};
use libp2p::{
    core::{
        upgrade::{read_length_prefixed, write_length_prefixed},
        ProtocolName,
    },
    request_response::RequestResponseCodec,
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Debug, Clone)]
pub struct BincodeCodec {
    /// Used for `max_size` parameter when reading Response Message
    /// Necessary in order to avoid DoS attacks
    /// Currently the size mostly depends on the max size of the FuelBlock
    max_response_size: usize,
}

impl BincodeCodec {
    pub fn new(max_block_size: usize) -> Self {
        Self {
            max_response_size: max_block_size,
        }
    }

    /// Helper method for decoding data
    /// Reusable across `RequestResponseCodec` and `GossipsubCodec`
    fn deserialize<'a, R: Deserialize<'a>>(&self, encoded_data: &'a [u8]) -> Result<R, io::Error> {
        bincode::deserialize(encoded_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn serialize<D: Serialize>(&self, data: &D) -> Result<Vec<u8>, io::Error> {
        bincode::serialize(&data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}

/// Since Bincode does not support async reads or writes out of the box
/// We prefix Request & Response Messages with the length of the data in bytes
/// We expect the substream to be properly closed when response channel is dropped.
/// Since the request protocol used here expects a response, the sender considers this
/// early close as a protocol violation which results in the connection being closed.
/// If the substream was not properly closed when dropped, the sender would instead
/// run into a timeout waiting for the response.
#[async_trait]
impl RequestResponseCodec for BincodeCodec {
    type Protocol = MessageExchangeBincodeProtocol;
    type Request = RequestMessage;
    type Response = IntermediateResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let encoded_data = read_length_prefixed(socket, MAX_REQUEST_SIZE).await?;

        self.deserialize(&encoded_data)
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let encoded_data = read_length_prefixed(socket, self.max_response_size).await?;

        self.deserialize(&encoded_data)
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        match bincode::serialize(&req) {
            Ok(encoded_data) => {
                write_length_prefixed(socket, encoded_data).await?;
                socket.close().await?;

                Ok(())
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        match bincode::serialize(&res) {
            Ok(encoded_data) => {
                write_length_prefixed(socket, encoded_data).await?;
                socket.close().await?;

                Ok(())
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }
}

impl GossipsubCodec for BincodeCodec {
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        let encoded_data = match data {
            GossipsubBroadcastRequest::ConsensusVote(vote) => bincode::serialize(&*vote),
            GossipsubBroadcastRequest::NewBlock(block) => bincode::serialize(&*block),
            GossipsubBroadcastRequest::NewTx(tx) => bincode::serialize(&*tx),
        };

        encoded_data.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_tag: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error> {
        let decoded_response = match gossipsub_tag {
            GossipTopicTag::NewTx => GossipsubMessage::NewTx(self.deserialize(encoded_data)?),
            GossipTopicTag::NewBlock => GossipsubMessage::NewBlock(self.deserialize(encoded_data)?),
            GossipTopicTag::ConsensusVote => {
                GossipsubMessage::ConsensusVote(self.deserialize(encoded_data)?)
            }
        };

        Ok(decoded_response)
    }
}

impl RequestResponseConverter for BincodeCodec {
    type IntermediateResponse = IntermediateResponse;
    type OutboundResponse = OutboundResponse;
    type ResponseMessage = ResponseMessage;

    fn convert_to_response(
        &self,
        inter_msg: &Self::IntermediateResponse,
    ) -> Result<Self::ResponseMessage, io::Error> {
        match inter_msg {
            IntermediateResponse::ResponseBlock(block_bytes) => Ok(ResponseMessage::ResponseBlock(
                self.deserialize(block_bytes)?,
            )),
        }
    }

    fn convert_to_intermediate(
        &self,
        res_msg: &Self::OutboundResponse,
    ) -> Result<Self::IntermediateResponse, io::Error> {
        match res_msg {
            OutboundResponse::ResponseBlock(sealed_block) => Ok(
                IntermediateResponse::ResponseBlock(self.serialize(&**sealed_block)?),
            ),
        }
    }
}

impl NetworkCodec for BincodeCodec {
    fn get_req_res_protocol(&self) -> <Self as RequestResponseCodec>::Protocol {
        MessageExchangeBincodeProtocol {}
    }
}

#[derive(Debug, Clone)]
pub struct MessageExchangeBincodeProtocol;

impl ProtocolName for MessageExchangeBincodeProtocol {
    fn protocol_name(&self) -> &[u8] {
        REQUEST_RESPONSE_PROTOCOL_ID
    }
}
