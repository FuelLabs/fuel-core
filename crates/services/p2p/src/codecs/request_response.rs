use std::io;

use crate::request_response::{
    messages::{
        RequestMessage,
        V1ResponseMessage,
        V2ResponseMessage,
    },
    protocols::RequestResponseProtocol,
};
use async_trait::async_trait;
use futures::{
    AsyncRead,
    AsyncReadExt,
    AsyncWriteExt,
};
use libp2p::request_response;
use strum::IntoEnumIterator as _;

use super::{
    Decode,
    Encode,
    Encoder,
    RequestResponseProtocols,
};

#[derive(Debug, Clone)]
pub struct RequestResponseMessageHandler<Format> {
    pub(crate) data_format: Format,
    /// Used for `max_size` parameter when reading Response Message
    /// Necessary in order to avoid DoS attacks
    /// Currently the size mostly depends on the max size of the Block
    pub(crate) max_response_size: usize,
}

/// Since Postcard does not support async reads or writes out of the box
/// We prefix Request & Response Messages with the length of the data in bytes
/// We expect the substream to be properly closed when response channel is dropped.
/// Since the request protocol used here expects a response, the sender considers this
/// early close as a protocol violation which results in the connection being closed.
/// If the substream was not properly closed when dropped, the sender would instead
/// run into a timeout waiting for the response.
#[async_trait]
impl<Format> request_response::Codec for RequestResponseMessageHandler<Format>
where
    Format: Encode<RequestMessage, Error = io::Error>
        + Decode<RequestMessage, Error = io::Error>
        + Encode<V1ResponseMessage, Error = io::Error>
        + Decode<V1ResponseMessage, Error = io::Error>
        + Encode<V2ResponseMessage, Error = io::Error>
        + Decode<V2ResponseMessage, Error = io::Error>
        + Send,
{
    type Protocol = RequestResponseProtocol;
    type Request = RequestMessage;
    type Response = V2ResponseMessage;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        socket: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut response = Vec::new();
        socket
            .take(self.max_response_size as u64)
            .read_to_end(&mut response)
            .await?;
        Format::decode(&response)
    }

    async fn read_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        socket: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut response = Vec::new();
        socket
            .take(self.max_response_size as u64)
            .read_to_end(&mut response)
            .await?;

        match protocol {
            RequestResponseProtocol::V1 => {
                let v1_response =
                    <Format as Decode<V1ResponseMessage>>::decode(&response)?;
                Ok(v1_response.into())
            }
            RequestResponseProtocol::V2 => {
                <Format as Decode<V2ResponseMessage>>::decode(&response)
            }
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
        let encoded_data = self.data_format.encode(&req)?;
        socket.write_all(&encoded_data.as_bytes()).await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        socket: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        match protocol {
            RequestResponseProtocol::V1 => {
                let v1_response: V1ResponseMessage = res.into();
                let res = self.data_format.encode(&v1_response)?;
                let res = res.as_bytes();
                socket.write_all(&res).await?;
            }
            RequestResponseProtocol::V2 => {
                let res = self.data_format.encode(&res)?;
                let res = res.as_bytes();
                socket.write_all(&res).await?;
            }
        };

        Ok(())
    }
}

impl<Codec> RequestResponseProtocols for Codec
where
    Codec: request_response::Codec<Protocol = RequestResponseProtocol>,
{
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as request_response::Codec>::Protocol> {
        // TODO: Iterating over versions in reverse order should prefer
        // peers to use V2 over V1 for exchanging messages. However, this is
        // not guaranteed by the specs for the `request_response` protocol.
        RequestResponseProtocol::iter().rev()
    }
}
