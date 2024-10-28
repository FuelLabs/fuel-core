use super::{
    bounded::BoundedCodec,
    unbounded::UnboundedCodec,
    DataFormat,
};

use serde::{
    Deserialize,
    Serialize,
};
use std::io;

#[derive(Clone)]
pub struct PostcardDataFormat;

impl DataFormat for PostcardDataFormat {
    type Error = io::Error;

    fn deserialize<'a, R: Deserialize<'a>>(
        &self,
        encoded_data: &'a [u8],
    ) -> Result<R, Self::Error> {
        postcard::from_bytes(encoded_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    fn serialize<D: Serialize>(&self, data: &D) -> Result<Vec<u8>, Self::Error> {
        postcard::to_stdvec(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}

impl BoundedCodec<PostcardDataFormat> {
    pub fn new(max_block_size: usize) -> Self {
        assert_ne!(
            max_block_size, 0,
            "PostcardCodec does not support zero block size"
        );

        Self {
            data_format: PostcardDataFormat,
            max_response_size: max_block_size,
        }
    }
}

impl UnboundedCodec<PostcardDataFormat> {
    pub fn new() -> Self {
        UnboundedCodec {
            data_format: PostcardDataFormat,
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {

    use fuel_core_types::blockchain::SealedBlockHeader;
    use libp2p::request_response::Codec;

    use super::*;
    use crate::{
        codecs::bounded::BoundedCodec,
        request_response::{
            messages::{
                RequestMessage,
                ResponseMessageErrorCode,
                V1ResponseMessage,
                V2ResponseMessage,
                MAX_REQUEST_SIZE,
            },
            protocols::RequestResponseProtocol,
        },
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
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&RequestResponseProtocol::V2, &mut buf, response)
            .await
            .expect("Valid Vec<SealedBlockHeader> should be serialized using v1");

        let deserialized = codec
            .read_response(&RequestResponseProtocol::V2, &mut buf.as_slice())
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
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&RequestResponseProtocol::V1, &mut buf, response)
            .await
            .expect("Valid Vec<SealedBlockHeader> should be serialized using v1");

        let deserialized = codec
            .read_response(&RequestResponseProtocol::V1, &mut buf.as_slice())
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
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&RequestResponseProtocol::V2, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized = codec
            .read_response(&RequestResponseProtocol::V2, &mut buf.as_slice())
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
    async fn codec__serialzation_roundtrip_using_v1_on_error_response_returns_predefined_error_code(
    ) {
        // Given
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1311
        // Change this to a different ResponseMessageErrorCode once these have been implemented.
        let response = V2ResponseMessage::SealedHeaders(Err(
            ResponseMessageErrorCode::ProtocolV1EmptyResponse,
        ));
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&RequestResponseProtocol::V1, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized = codec
            .read_response(&RequestResponseProtocol::V1, &mut buf.as_slice())
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
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);
        let mut buf = Vec::with_capacity(1024);

        // When
        codec
            .write_response(&RequestResponseProtocol::V1, &mut buf, response.clone())
            .await
            .expect("Valid Vec<SealedBlockHeader> is serialized using v1");

        let deserialized_as_v1 =
            // We cannot access the codec trait from an old node here, 
            // so we deserialize directly using the `V1ResponseMessage` type.
            codec.data_format.deserialize::<V1ResponseMessage>(&buf).expect("Deserialization as V1ResponseMessage should succeed");

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
        let mut codec: BoundedCodec<PostcardDataFormat> = BoundedCodec::new(1024);

        // When
        let buf = codec
            .data_format
            .serialize(&response)
            .expect("Serialization as V1ResponseMessage should succeed");
        let deserialized = codec
            .read_response(&RequestResponseProtocol::V1, &mut buf.as_slice())
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
