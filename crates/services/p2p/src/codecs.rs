pub mod gossipsub;
pub mod postcard;
pub mod request_response;

use crate::gossipsub::messages::GossipTopicTag;
use libp2p::request_response as libp2p_request_response;

use std::{
    borrow::Cow,
    io,
};

// TODO: https://github.com/FuelLabs/fuel-core/issues/2403
// This trait is largely a copy-paste from the storage crate.
// It would be best to have this trait in a separate crate that both storage and p2p can depend on.
pub trait Encoder: Send {
    /// Returns the serialized object as a slice.
    fn as_bytes(&self) -> Cow<[u8]>;
}

/// The trait encodes the type to the bytes and passes it to the `Encoder`,
/// which stores it and provides a reference to it. That allows gives more
/// flexibility and more performant encoding, allowing the use of slices and arrays
/// instead of vectors in some cases. Since the [`Encoder`] returns `Cow<[u8]>`,
/// it is always possible to take ownership of the serialized value.
// TODO: https://github.com/FuelLabs/fuel-core/issues/2403
// This trait is largely a copy-paste from the storage crate.
// It would be best to have this trait in a separate crate that both storage and p2p can depend on.
pub trait Encode<T: ?Sized> {
    type Error;
    /// The encoder type that stores serialized object.
    type Encoder<'a>: Encoder
    where
        T: 'a;

    /// Encodes the object to the bytes and passes it to the `Encoder`.
    fn encode<'b>(&self, t: &'b T) -> Result<Self::Encoder<'b>, Self::Error>;
}

/// The trait decodes the type from the bytes.
// TODO: https://github.com/FuelLabs/fuel-core/issues/2403
// This trait is largely a copy-paste from the storage crate.
// It would be best to have this trait in a separate crate that both storage and p2p can depend on.
pub trait Decode<T> {
    type Error;
    /// Decodes the type `T` from the bytes.
    fn decode(&self, bytes: &[u8]) -> Result<T, Self::Error>;
}

impl<'a> Encoder for Cow<'a, [u8]> {
    fn as_bytes(&self) -> Cow<'_, [u8]> {
        match self {
            Cow::Borrowed(borrowed) => Cow::Borrowed(borrowed),
            Cow::Owned(owned) => Cow::Borrowed(owned.as_ref()),
        }
    }
}

/// Implement this in order to handle serialization & deserialization of Gossipsub messages
pub trait GossipsubCodec {
    type RequestMessage;
    type ResponseMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error>;

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_topic: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error>;
}

pub trait RequestResponseProtocols: libp2p_request_response::Codec {
    /// Returns RequestResponse's Protocol
    /// Needed for initialization of RequestResponse Behaviour
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as libp2p_request_response::Codec>::Protocol>;
}
