use super::{
    GossipsubCodec,
    NetworkCodec,
};
use crate::{
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    request_response::messages::{
        LegacyResponseMessage,
        RequestMessage,
        ResponseMessage,
        REQUEST_RESPONSE_PROTOCOL_ID,
        REQUEST_RESPONSE_WITH_ERROR_CODES_PROTOCOL_ID,
    },
};
use async_trait::async_trait;
use futures::{
    AsyncRead,
    AsyncReadExt,
    AsyncWriteExt,
};
use libp2p::request_response;
use serde::{
    Deserialize,
    Serialize,
};
use std::io;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// Helper method for decoding data
/// Reusable across `RequestResponseCodec` and `GossipsubCodec`
fn deserialize<'a, R: Deserialize<'a>>(encoded_data: &'a [u8]) -> Result<R, io::Error> {
    postcard::from_bytes(encoded_data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

fn serialize<D: Serialize>(data: &D) -> Result<Vec<u8>, io::Error> {
    postcard::to_stdvec(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

#[derive(Debug, Clone)]
pub struct PostcardCodec {
    /// Used for `max_size` parameter when reading Response Message
    /// Necessary in order to avoid DoS attacks
    /// Currently the size mostly depends on the max size of the Block
    max_response_size: usize,
}

impl PostcardCodec {
    pub fn new(max_block_size: usize) -> Self {
        assert_ne!(
            max_block_size, 0,
            "PostcardCodec does not support zero block size"
        );

        Self {
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
impl request_response::Codec for PostcardCodec {
    type Protocol = PostcardProtocol;
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
        let mut response = Vec::new();
        socket
            .take(self.max_response_size as u64)
            .read_to_end(&mut response)
            .await?;
        deserialize(&response)
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
            PostcardProtocol::V1 => {
                let legacy_response = deserialize::<LegacyResponseMessage>(&response)?;
                Ok(legacy_response.into())
            }
            PostcardProtocol::V2 => deserialize::<ResponseMessage>(&response),
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
        let encoded_data = serialize(&req)?;
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
            PostcardProtocol::V1 => {
                let legacy_response: LegacyResponseMessage = res.into();
                serialize(&legacy_response)?
            }
            PostcardProtocol::V2 => serialize(&res)?,
        };
        socket.write_all(&encoded_data).await?;
        Ok(())
    }
}

impl GossipsubCodec for PostcardCodec {
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
            GossipTopicTag::NewTx => GossipsubMessage::NewTx(deserialize(encoded_data)?),
        };

        Ok(decoded_response)
    }
}

// TODO: Remove this NetworkCodec
impl NetworkCodec for PostcardCodec {
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as request_response::Codec>::Protocol> {
        // TODO: Iterating over versions in reverse order should prefer
        // peers to use V2 over V1 for exchanging messages. However, this is
        // not guaranteed by the specs for the `request_response` protocol.
        PostcardProtocol::iter().rev()
    }
}

#[derive(Debug, Clone, EnumIter)]
pub enum PostcardProtocol {
    V1,
    V2,
}

impl Default for PostcardProtocol {
    fn default() -> Self {
        PostcardProtocol::V1
    }
}

impl AsRef<str> for PostcardProtocol {
    fn as_ref(&self) -> &str {
        match self {
            PostcardProtocol::V1 => REQUEST_RESPONSE_PROTOCOL_ID,
            PostcardProtocol::V2 => REQUEST_RESPONSE_WITH_ERROR_CODES_PROTOCOL_ID,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_response::messages::MAX_REQUEST_SIZE;

    #[test]
    fn test_request_size_fits() {
        let arbitrary_range = 2..6;
        let m = RequestMessage::Transactions(arbitrary_range);
        assert!(postcard::to_stdvec(&m).unwrap().len() <= MAX_REQUEST_SIZE);
    }
}
