pub mod bincode;

use crate::{
    gossipsub::messages::GossipsubMessage as FuelGossipsubMessage,
    request_response::messages::{RequestMessage, ResponseMessage},
};
use libp2p::request_response::RequestResponseCodec;
use std::io;

/// Implement this in order to handle serialization & deserialization of Gossipsub messages
pub trait GossipsubCodec {
    type Message;

    fn encode(&self, data: Self::Message) -> Result<Vec<u8>, io::Error>;

    fn decode(&self, encoded_data: &[u8]) -> Result<Self::Message, io::Error>;
}

/// Main Codec trait
/// Needs to be implemented and provided to FuelBehaviour
pub trait NetworkCodec:
    GossipsubCodec<Message = FuelGossipsubMessage>
    + RequestResponseCodec<Request = RequestMessage, Response = ResponseMessage>
    + Clone
    + Send
    + 'static
{
    /// Returns RequestResponse's Protocol
    /// Needed for initialization of RequestResponse Behaviour
    fn get_req_res_protocol(&self) -> <Self as RequestResponseCodec>::Protocol;
}
