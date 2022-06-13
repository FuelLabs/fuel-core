use super::{GossipsubCodec, NetworkCodec};
use crate::{
    gossipsub::messages::GossipsubMessage,
    request_response::messages::{
        RequestMessage, ResponseMessage, MAX_REQUEST_SIZE, REQUEST_RESPONSE_PROTOCOL_ID,
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
    type Response = ResponseMessage;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let encoded_data = read_length_prefixed(socket, MAX_REQUEST_SIZE).await?;

        match bincode::deserialize(&encoded_data) {
            Ok(decoded_data) => Ok(decoded_data),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
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

        match bincode::deserialize(&encoded_data) {
            Ok(decoded_data) => Ok(decoded_data),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
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
    type Message = GossipsubMessage;

    fn encode(&self, data: Self::Message) -> Result<Vec<u8>, io::Error> {
        bincode::serialize(&data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn decode(&self, encoded_data: &[u8]) -> Result<Self::Message, io::Error> {
        bincode::deserialize(encoded_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
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
