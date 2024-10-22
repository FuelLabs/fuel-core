pub mod postcard;

use crate::{
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    request_response::messages::{
        RequestMessage,
        V2ResponseMessage,
    },
};
use libp2p::request_response;
use std::io;

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

// TODO: https://github.com/FuelLabs/fuel-core/issues/2368
// Remove this trait
/// Main Codec trait
/// Needs to be implemented and provided to FuelBehaviour
pub trait NetworkCodec:
    GossipsubCodec<
        RequestMessage = GossipsubBroadcastRequest,
        ResponseMessage = GossipsubMessage,
    > + request_response::Codec<Request = RequestMessage, Response = V2ResponseMessage>
    + Clone
    + Send
    + 'static
{
    /// Returns RequestResponse's Protocol
    /// Needed for initialization of RequestResponse Behaviour
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as request_response::Codec>::Protocol>;
}
