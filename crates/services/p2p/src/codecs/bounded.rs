use std::{
    io,
    marker::PhantomData,
};

use async_trait::async_trait;
use futures::{
    AsyncRead,
    AsyncReadExt,
    AsyncWriteExt,
};
use libp2p::request_response;

use crate::{
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    request_response::messages::{
        RequestMessage,
        V1ResponseMessage,
        V2ResponseMessage,
    },
};
use strum::IntoEnumIterator;

use super::{
    postcard::RequestResponseProtocol,
    DataFormatCodec,
    GossipsubCodec,
    RequestResponseProtocols,
};

#[derive(Debug, Clone)]
pub struct BoundedCodec<Format> {
    _format: PhantomData<Format>,
    /// Used for `max_size` parameter when reading Response Message
    /// Necessary in order to avoid DoS attacks
    /// Currently the size mostly depends on the max size of the Block
    max_response_size: usize,
}

impl<Format> BoundedCodec<Format> {
    pub fn new(max_block_size: usize) -> Self {
        assert_ne!(
            max_block_size, 0,
            "PostcardCodec does not support zero block size"
        );

        Self {
            _format: PhantomData,
            max_response_size: max_block_size,
        }
    }
}

/// Since Postcard does not support async reads or writes out of the box
/// We prefix Request & Response Messages with the length of the data in bytes
/// We expect the substream to be properly closed when response channel is dropped.
/// Since the request protocol used here expects a response, the sender considers this
/// early close as a protocol violation which results in the connection being closed.
/// If the substream was not properly closed when dropped, the sender would instead
/// run into a timeout waiting for the response.
#[async_trait]
impl<Format> request_response::Codec for BoundedCodec<Format>
where
    Format: DataFormatCodec<Error = io::Error> + Send,
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
        Format::deserialize(&response)
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
                let v1_response = Format::deserialize::<V1ResponseMessage>(&response)?;
                Ok(v1_response.into())
            }
            RequestResponseProtocol::V2 => {
                Format::deserialize::<V2ResponseMessage>(&response)
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
        let encoded_data = Format::serialize(&req)?;
        socket.write_all(&encoded_data).await?;
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
        let encoded_data = match protocol {
            RequestResponseProtocol::V1 => {
                let v1_response: V1ResponseMessage = res.into();
                Format::serialize(&v1_response)?
            }
            RequestResponseProtocol::V2 => Format::serialize(&res)?,
        };
        socket.write_all(&encoded_data).await?;
        Ok(())
    }
}

impl<Format> GossipsubCodec for BoundedCodec<Format>
where
    Format: DataFormatCodec<Error = io::Error> + Send,
{
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        let encoded_data = match data {
            GossipsubBroadcastRequest::NewTx(tx) => postcard::to_stdvec(&*tx),
        };

        encoded_data.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_tag: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error> {
        let decoded_response = match gossipsub_tag {
            GossipTopicTag::NewTx => {
                GossipsubMessage::NewTx(Format::deserialize(encoded_data)?)
            }
        };

        Ok(decoded_response)
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
