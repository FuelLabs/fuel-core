use crate::{
    codecs::NetworkCodec,
    config::P2PConfig,
    discovery::{
        DiscoveryBehaviour,
        DiscoveryConfig,
        DiscoveryEvent,
    },
    gossipsub::{
        self,
        messages::{
            GossipsubBroadcastRequest,
            GossipsubMessage as FuelGossipsubMessage,
        },
        topics::{
            GossipTopic,
            GossipsubTopics,
        },
    },
    peer_info::{
        PeerInfo,
        PeerInfoBehaviour,
        PeerInfoEvent,
    },
    request_response::messages::{
        IntermediateResponse,
        OutboundResponse,
        RequestMessage,
        ResponseChannelItem,
        ResponseError,
        ResponseMessage,
    },
};
use libp2p::{
    gossipsub::{
        error::{
            PublishError,
            SubscriptionError,
        },
        Gossipsub,
        GossipsubEvent,
        MessageId,
        TopicHash,
    },
    request_response::{
        ProtocolSupport,
        RequestId,
        RequestResponse,
        RequestResponseConfig,
        RequestResponseEvent,
        RequestResponseMessage,
        ResponseChannel,
    },
    swarm::{
        NetworkBehaviour,
        NetworkBehaviourAction,
        NetworkBehaviourEventProcess,
        PollParameters,
    },
    NetworkBehaviour,
    PeerId,
};
use std::{
    collections::{
        HashMap,
        VecDeque,
    },
    task::{
        Context,
        Poll,
    },
};
use tracing::{
    debug,
    warn,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum FuelBehaviourEvent {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    PeerIdentified(PeerId),
    PeerInfoUpdated(PeerId),
    GossipsubMessage {
        peer_id: PeerId,
        topic_hash: TopicHash,
        message: FuelGossipsubMessage,
    },
    RequestMessage {
        request_id: RequestId,
        request_message: RequestMessage,
    },
}

/// Holds additional Network data for FuelBehavior
#[derive(Debug)]
struct NetworkMetadata {
    gossipsub_topics: GossipsubTopics,
}

/// Handles all p2p protocols needed for Fuel.
#[derive(NetworkBehaviour)]
#[behaviour(
    out_event = "FuelBehaviourEvent",
    poll_method = "poll",
    event_process = true
)]
pub struct FuelBehaviour<Codec: NetworkCodec> {
    /// Node discovery
    discovery: DiscoveryBehaviour,

    /// Identify and periodically ping nodes
    peer_info: PeerInfoBehaviour,

    /// Message propagation for p2p
    gossipsub: Gossipsub,

    /// RequestResponse protocol
    request_response: RequestResponse<Codec>,

    /// Holds the Sender(s) part of the Oneshot Channel from the NetworkOrchestrator
    /// Once the ResponseMessage is received from the p2p Network
    /// It will send it to the NetworkOrchestrator via its unique Sender
    #[behaviour(ignore)]
    outbound_requests_table: HashMap<RequestId, ResponseChannelItem>,

    /// Holds the ResponseChannel(s) for the inbound requests from the p2p Network
    /// Once the Response is prepared by the NetworkOrchestrator
    /// It will send it to the specified Peer via its unique ResponseChannel
    #[behaviour(ignore)]
    inbound_requests_table: HashMap<RequestId, ResponseChannel<IntermediateResponse>>,

    /// Double-ended queue of FuelBehaviour Events
    #[behaviour(ignore)]
    events: VecDeque<FuelBehaviourEvent>,

    /// NetworkCodec used as <GossipsubCodec> for encoding and decoding of Gossipsub messages
    #[behaviour(ignore)]
    codec: Codec,

    /// Stores additional p2p network info
    #[behaviour(ignore)]
    network_metadata: NetworkMetadata,
}

impl<Codec: NetworkCodec> FuelBehaviour<Codec> {
    pub fn new(p2p_config: &P2PConfig, codec: Codec) -> Self {
        let local_public_key = p2p_config.local_keypair.public();
        let local_peer_id = PeerId::from_public_key(&local_public_key);

        let discovery_config = {
            let mut discovery_config =
                DiscoveryConfig::new(local_peer_id, p2p_config.network_name.clone());

            discovery_config
                .enable_mdns(p2p_config.enable_mdns)
                .discovery_limit(p2p_config.max_peers_connected)
                .allow_private_addresses(p2p_config.allow_private_addresses)
                .with_bootstrap_nodes(p2p_config.bootstrap_nodes.clone())
                .enable_random_walk(p2p_config.enable_random_walk);

            if let Some(duration) = p2p_config.connection_idle_timeout {
                discovery_config.set_connection_idle_timeout(duration);
            }

            discovery_config
        };

        let peer_info = PeerInfoBehaviour::new(local_public_key, p2p_config);

        let req_res_protocol =
            std::iter::once((codec.get_req_res_protocol(), ProtocolSupport::Full));

        let mut req_res_config = RequestResponseConfig::default();
        req_res_config.set_request_timeout(p2p_config.set_request_timeout);
        req_res_config.set_connection_keep_alive(p2p_config.set_connection_keep_alive);

        let request_response =
            RequestResponse::new(codec.clone(), req_res_protocol, req_res_config);

        let gossipsub_topics = GossipsubTopics::new(&p2p_config.network_name);
        let network_metadata = NetworkMetadata { gossipsub_topics };

        Self {
            discovery: discovery_config.finish(),
            gossipsub: gossipsub::build_gossipsub(&p2p_config.local_keypair, p2p_config),
            peer_info,
            request_response,

            outbound_requests_table: HashMap::default(),
            inbound_requests_table: HashMap::default(),
            events: VecDeque::default(),
            codec,
            network_metadata,
        }
    }

    // Currently only used in testing hence `allow`
    #[allow(dead_code)]
    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peer_info.get_peer_info(peer_id)
    }

    pub fn get_peers(&self) -> &HashMap<PeerId, PeerInfo> {
        self.peer_info.peers()
    }

    pub fn publish_message(
        &mut self,
        message: GossipsubBroadcastRequest,
    ) -> Result<MessageId, PublishError> {
        let topic = self
            .network_metadata
            .gossipsub_topics
            .get_gossipsub_topic(&message);
        match self.codec.encode(message) {
            Ok(encoded_data) => self.gossipsub.publish(topic, encoded_data),
            Err(e) => Err(PublishError::TransformFailed(e)),
        }
    }

    pub fn subscribe_to_topic(
        &mut self,
        topic: &GossipTopic,
    ) -> Result<bool, SubscriptionError> {
        self.gossipsub.subscribe(topic)
    }

    pub fn send_request_msg(
        &mut self,
        message_request: RequestMessage,
        peer_id: PeerId,
        channel_item: ResponseChannelItem,
    ) -> RequestId {
        let request_id = self
            .request_response
            .send_request(&peer_id, message_request);

        self.outbound_requests_table
            .insert(request_id, channel_item);

        request_id
    }

    pub fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: OutboundResponse,
    ) -> Result<(), ResponseError> {
        match (
            self.codec.convert_to_intermediate(&message),
            self.inbound_requests_table.remove(&request_id),
        ) {
            (Ok(message), Some(channel)) => {
                if self
                    .request_response
                    .send_response(channel, message)
                    .is_err()
                {
                    debug!("Failed to send ResponseMessage for {:?}", request_id);
                    return Err(ResponseError::SendingResponseFailed)
                }
            }
            (Ok(_), None) => {
                debug!("ResponseChannel for {:?} does not exist!", request_id);
                return Err(ResponseError::ResponseChannelDoesNotExist)
            }
            (Err(e), _) => {
                debug!("Failed to convert to IntermediateResponse with {:?}", e);
                return Err(ResponseError::ConversionToIntermediateFailed)
            }
        }

        Ok(())
    }

    // report events to the swarm
    fn poll(
        &mut self,
        _cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self as NetworkBehaviour>::OutEvent,
            <Self as NetworkBehaviour>::ConnectionHandler,
        >,
    > {
        match self.events.pop_front() {
            Some(event) => Poll::Ready(NetworkBehaviourAction::GenerateEvent(event)),
            _ => Poll::Pending,
        }
    }

    /// Getter for outbound_requests_table
    /// Used only in testing in `service.rs`
    #[allow(dead_code)]
    pub(super) fn get_outbound_requests_table(
        &self,
    ) -> &HashMap<RequestId, ResponseChannelItem> {
        &self.outbound_requests_table
    }
}

impl<Codec: NetworkCodec> NetworkBehaviourEventProcess<DiscoveryEvent>
    for FuelBehaviour<Codec>
{
    fn inject_event(&mut self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Connected(peer_id, addresses) => {
                self.peer_info.insert_peer_addresses(&peer_id, addresses);

                self.events
                    .push_back(FuelBehaviourEvent::PeerConnected(peer_id));
            }
            DiscoveryEvent::Disconnected(peer_id) => self
                .events
                .push_back(FuelBehaviourEvent::PeerDisconnected(peer_id)),

            _ => {}
        }
    }
}

impl<Codec: NetworkCodec> NetworkBehaviourEventProcess<PeerInfoEvent>
    for FuelBehaviour<Codec>
{
    fn inject_event(&mut self, event: PeerInfoEvent) {
        match event {
            PeerInfoEvent::PeerIdentified { peer_id, addresses } => {
                for address in addresses {
                    self.discovery.add_address(&peer_id, address.clone());
                }

                self.events
                    .push_back(FuelBehaviourEvent::PeerIdentified(peer_id));
            }

            PeerInfoEvent::PeerInfoUpdated { peer_id } => self
                .events
                .push_back(FuelBehaviourEvent::PeerInfoUpdated(peer_id)),
        }
    }
}

impl<Codec: NetworkCodec> NetworkBehaviourEventProcess<GossipsubEvent>
    for FuelBehaviour<Codec>
{
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message {
            propagation_source,
            message,
            message_id: _,
        } = message
        {
            if let Some(correct_topic) = self
                .network_metadata
                .gossipsub_topics
                .get_gossipsub_tag(&message.topic)
            {
                match self.codec.decode(&message.data, correct_topic) {
                    Ok(decoded_message) => {
                        self.events.push_back(FuelBehaviourEvent::GossipsubMessage {
                            peer_id: propagation_source,
                            topic_hash: message.topic,
                            message: decoded_message,
                        })
                    }
                    Err(err) => {
                        warn!(target: "fuel-libp2p", "Failed to decode a message: {:?} with error: {:?}", &message.data, err);
                    }
                }
            } else {
                warn!(target: "fuel-libp2p", "GossipTopicTag does not exist for {:?}", &message.topic);
            }
        }
    }
}

impl<Codec: NetworkCodec>
    NetworkBehaviourEventProcess<
        RequestResponseEvent<RequestMessage, IntermediateResponse>,
    > for FuelBehaviour<Codec>
{
    fn inject_event(
        &mut self,
        event: RequestResponseEvent<RequestMessage, IntermediateResponse>,
    ) {
        match event {
            RequestResponseEvent::Message { message, .. } => match message {
                RequestResponseMessage::Request {
                    request,
                    channel,
                    request_id,
                } => {
                    self.inbound_requests_table.insert(request_id, channel);
                    self.events.push_back(FuelBehaviourEvent::RequestMessage {
                        request_id,
                        request_message: request,
                    })
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => {
                    match (
                        self.outbound_requests_table.remove(&request_id),
                        self.codec.convert_to_response(&response),
                    ) {
                        (
                            Some(ResponseChannelItem::ResponseBlock(channel)),
                            Ok(ResponseMessage::ResponseBlock(block)),
                        ) => {
                            if channel.send(block).is_err() {
                                debug!(
                                    "Failed to send through the channel for {:?}",
                                    request_id
                                );
                            }
                        }

                        (Some(_), Err(e)) => {
                            debug!("Failed to convert IntermediateResponse into a ResponseMessage {:?} with {:?}", response, e);
                        }
                        (None, Ok(_)) => {
                            debug!("Send channel not found for {:?}", request_id);
                        }
                        _ => {}
                    }
                }
            },
            RequestResponseEvent::InboundFailure {
                peer,
                error,
                request_id,
            } => {
                debug!(
                    "RequestResponse inbound error for peer: {:?} with id: {:?} and error: {:?}",
                    peer, request_id, error
                );
            }
            RequestResponseEvent::OutboundFailure {
                peer,
                error,
                request_id,
            } => {
                debug!(
                    "RequestResponse outbound error for peer: {:?} with id: {:?} and error: {:?}",
                    peer, request_id, error
                );

                let _ = self.outbound_requests_table.remove(&request_id);
            }
            _ => {}
        }
    }
}
