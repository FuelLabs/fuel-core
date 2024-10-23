pub mod postcard;

use crate::{
    gossipsub::messages::GossipTopicTag,
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
pub trait RequestResponseCodec:
    request_response::Codec<Request = RequestMessage, Response = V2ResponseMessage>
{
    /// Returns RequestResponse's Protocol
    /// Needed for initialization of RequestResponse Behaviour
    fn get_req_res_protocols(
        &self,
    ) -> impl Iterator<Item = <Self as request_response::Codec>::Protocol>;
}
