pub mod postcard;

use crate::{
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    request_response::messages::{
        NetworkResponse,
        OutboundResponse,
        RequestMessage,
        ResponseMessage,
    },
};
use libp2p::request_response::Codec as RequestResponseCodec;
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

pub trait RequestResponseConverter {
    /// Response that is ready to be converted into NetworkResponse
    type OutboundResponse;
    /// Response that is sent over the network
    type NetworkResponse;
    /// Final Response Message deserialized from IntermediateResponse
    type ResponseMessage;

    fn convert_to_network_response(
        &self,
        res_msg: &Self::OutboundResponse,
    ) -> Result<Self::NetworkResponse, io::Error>;

    fn convert_to_response(
        &self,
        inter_msg: &Self::NetworkResponse,
    ) -> Result<Self::ResponseMessage, io::Error>;
}

/// Main Codec trait
/// Needs to be implemented and provided to FuelBehaviour
pub trait NetworkCodec:
    GossipsubCodec<
        RequestMessage = GossipsubBroadcastRequest,
        ResponseMessage = GossipsubMessage,
    > + RequestResponseCodec<Request = RequestMessage, Response = NetworkResponse>
    + RequestResponseConverter<
        NetworkResponse = NetworkResponse,
        OutboundResponse = OutboundResponse,
        ResponseMessage = ResponseMessage,
    > + Clone
    + Send
    + 'static
{
    /// Returns RequestResponse's Protocol
    /// Needed for initialization of RequestResponse Behaviour
    fn get_req_res_protocol(&self) -> <Self as RequestResponseCodec>::Protocol;
}
