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
        RequestMessage,
        V1ResponseMessage,
        V2ResponseMessage,
        V1_REQUEST_RESPONSE_PROTOCOL_ID,
        V2_REQUEST_RESPONSE_PROTOCOL_ID,
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
                let v1_response = deserialize::<V1ResponseMessage>(&response)?;
                Ok(v1_response.into())
            }
            PostcardProtocol::V2 => deserialize::<V2ResponseMessage>(&response),
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
                let v1_response: V1ResponseMessage = res.into();
                serialize(&v1_response)?
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

impl NetworkCodec for PostcardCodec {
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as request_response::Codec>::Protocol> {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/2374
        // Iterating over versions in reverse order should prefer
        // peers to use V2 over V1 for exchanging messages. However, this is
        // not guaranteed by the specs for the `request_response` protocol,
        // and it should be tested.
        PostcardProtocol::iter().rev()
    }
}

#[derive(Debug, Clone, EnumIter)]
pub enum PostcardProtocol {
    V1,
    V2,
}

impl AsRef<str> for PostcardProtocol {
    fn as_ref(&self) -> &str {
        match self {
            PostcardProtocol::V1 => V1_REQUEST_RESPONSE_PROTOCOL_ID,
            PostcardProtocol::V2 => V2_REQUEST_RESPONSE_PROTOCOL_ID,
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use fuel_core_types::blockchain::SealedBlockHeader;
    use request_response::Codec as _;

    use super::*;
    use crate::request_response::messages::{
        ResponseMessageErrorCode,
        MAX_REQUEST_SIZE,
    };

    #[test]
    fn test_request_size_fits() {
        let arbitrary_range = 2..6;
        let m = RequestMessage::Transactions(arbitrary_range);
        assert!(postcard::to_stdvec(&m).unwrap().len() <= MAX_REQUEST_SIZE);
    }

    #[tokio::test]
    async fn codec__serialization_roundtrip_using_v2_on_successful_response_returns_original_value(
    ) {
        // Given
        let sealed_block_headers = vec![SealedBlockHeader::default()];
        let response = V2ResponseMessage::SealedHeaders(Ok(sealed_block_headers.clone()));
        let mut codec = PostcardCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&PostcardProtocol::V2, &mut buf, response)
            .await
            .expect("Valid Vec<SealedBlockHeader> should be serialized using v1");

        let deserialized = codec
            .read_response(&PostcardProtocol::V2, &mut buf.as_slice())
            .await
            .expect("Valid Vec<SealedBlockHeader> should be deserialized using v1");

        // Then
        assert!(matches!(
            deserialized,
            V2ResponseMessage::SealedHeaders(Ok(sealed_headers)) if sealed_headers == sealed_block_headers
        ));
    }

    #[tokio::test]
    async fn codec__serialization_roundtrip_using_v1_on_successful_response_returns_original_value(
    ) {
        // Given
        let sealed_block_headers = vec![SealedBlockHeader::default()];
        let response = V2ResponseMessage::SealedHeaders(Ok(sealed_block_headers.clone()));
        let mut codec = PostcardCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&PostcardProtocol::V1, &mut buf, response)
            .await
            .expect("Valid Vec<SealedBlockHeader> should be serialized using v1");

        let deserialized = codec
            .read_response(&PostcardProtocol::V1, &mut buf.as_slice())
            .await
            .expect("Valid Vec<SealedBlockHeader> should be deserialized using v1");

        // Then
        assert!(
            matches!(deserialized, V2ResponseMessage::SealedHeaders(Ok(sealed_headers)) if sealed_headers == sealed_block_headers)
        );
    }

    #[tokio::test]
    async fn codec__serialization_roundtrip_using_v2_on_error_response_returns_original_value(
    ) {
        // Given
        let response = V2ResponseMessage::SealedHeaders(Err(
            ResponseMessageErrorCode::ProtocolV1EmptyResponse,
        ));
        let mut codec = PostcardCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&PostcardProtocol::V2, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized = codec
            .read_response(&PostcardProtocol::V2, &mut buf.as_slice())
            .await
            .expect("Valid Vec<SealedBlockHeader> is deserialized using v1");

        // Then
        assert!(matches!(
            deserialized,
            V2ResponseMessage::SealedHeaders(Err(
                ResponseMessageErrorCode::ProtocolV1EmptyResponse
            ))
        ));
    }

    #[tokio::test]
    async fn codec__serialization_roundtrip_using_v1_on_error_response_returns_predefined_error_code(
    ) {
        // Given
        let response = V2ResponseMessage::SealedHeaders(Err(
            ResponseMessageErrorCode::RequestedRangeTooLarge,
        ));
        let mut codec = PostcardCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&PostcardProtocol::V1, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized = codec
            .read_response(&PostcardProtocol::V1, &mut buf.as_slice())
            .await
            .expect("Valid Vec<SealedBlockHeader> is deserialized using v1");

        // Then
        assert!(matches!(
            deserialized,
            V2ResponseMessage::SealedHeaders(Err(
                ResponseMessageErrorCode::ProtocolV1EmptyResponse
            ))
        ));
    }

    #[tokio::test]
    async fn codec__write_response_is_backwards_compatible_with_v1() {
        // Given
        let response = V2ResponseMessage::SealedHeaders(Err(
            ResponseMessageErrorCode::ProtocolV1EmptyResponse,
        ));
        let mut codec = PostcardCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&PostcardProtocol::V1, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized_as_v1 =
            // We cannot access the codec trait from an old node here, 
            // so we deserialize directly using the `V1ResponseMessage` type.
            deserialize::<V1ResponseMessage>(&buf).expect("Deserialization as V1ResponseMessage should succeed");

        // Then
        assert!(matches!(
            deserialized_as_v1,
            V1ResponseMessage::SealedHeaders(None)
        ));
    }

    #[tokio::test]
    async fn codec__read_response_is_backwards_compatible_with_v1() {
        // Given
        let response = V1ResponseMessage::SealedHeaders(None);
        let mut codec = PostcardCodec::new(1024);

        // When
        let buf = serialize(&response)
            .expect("Serialization as V1ResponseMessage should succeed");
        let deserialized = codec
            .read_response(&PostcardProtocol::V1, &mut buf.as_slice())
            .await
            .expect("Valid Vec<SealedBlockHeader> is deserialized using v1");

        // Then
        assert!(matches!(
            deserialized,
            V2ResponseMessage::SealedHeaders(Err(
                ResponseMessageErrorCode::ProtocolV1EmptyResponse
            ))
        ));
    }
}
