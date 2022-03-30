use super::messages::{RequestMessage, ResponseMessage};
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

pub const REQUEST_RESPONSE_PROTOCOL_ID: &[u8] = b"/fuel/req_res/0.0.1";
/// Max Size in Bytes of the messages
// todo: this will be defined once Request & Response Messages are clearly defined
// it should be the biggest field of respective enum
const MAX_REQUEST_SIZE: usize = 100;
const MAX_RESPONSE_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub struct MessageExchangeBincodeProtocol;

impl ProtocolName for MessageExchangeBincodeProtocol {
    fn protocol_name(&self) -> &[u8] {
        REQUEST_RESPONSE_PROTOCOL_ID
    }
}

#[derive(Debug, Clone)]
pub struct MessageExchangeBincodeCodec {}

/// Since Bincode does not support async reads or writes out of the box
/// We prefix Request & Response Messages with the length of the data in bytes
/// We expect the substream to be properly closed when response channel is dropped.
/// Since the request protocol used here expects a response, the sender considers this
/// early close as a protocol violation which results in the connection being closed.
/// If the substream were not properly closed when dropped, the sender would instead
/// run into a timeout waiting for the response.
#[async_trait]
impl RequestResponseCodec for MessageExchangeBincodeCodec {
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
        let encoded_data = read_length_prefixed(socket, MAX_RESPONSE_SIZE).await?;

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
