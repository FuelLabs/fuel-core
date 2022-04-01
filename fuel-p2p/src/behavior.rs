use crate::{
    config::{P2PConfig, REQ_RES_TIMEOUT},
    discovery::{DiscoveryBehaviour, DiscoveryConfig, DiscoveryEvent},
    gossipsub,
    peer_info::{PeerInfo, PeerInfoBehaviour, PeerInfoEvent},
    request_response::{
        codec::{MessageExchangeBincodeCodec, MessageExchangeBincodeProtocol},
        messages::{ReqResNetworkError, RequestMessage, ResponseError, ResponseMessage},
    },
    service::GossipTopic,
};
use libp2p::{
    gossipsub::{
        error::{PublishError, SubscriptionError},
        Gossipsub, GossipsubEvent, MessageId, TopicHash,
    },
    identity::Keypair,
    request_response::{
        ProtocolSupport, RequestId, RequestResponse, RequestResponseConfig, RequestResponseEvent,
        RequestResponseMessage, ResponseChannel,
    },
    swarm::{
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
    },
    NetworkBehaviour, PeerId,
};
use std::{
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tracing::debug;

#[derive(Debug)]
pub enum FuelBehaviourEvent {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    PeerIdentified(PeerId),
    PeerInfoUpdated(PeerId),
    GossipsubMessage {
        peer_id: PeerId,
        topic_hash: TopicHash,
        message: Vec<u8>,
    },
    RequestMessage {
        request_id: RequestId,
        request_message: RequestMessage,
    },
}

/// Handles all p2p protocols needed for Fuel.
#[derive(NetworkBehaviour)]
#[behaviour(
    out_event = "FuelBehaviourEvent",
    poll_method = "poll",
    event_process = true
)]
pub struct FuelBehaviour {
    /// Node discovery
    discovery: DiscoveryBehaviour,

    /// Identify and periodically ping nodes
    peer_info: PeerInfoBehaviour,

    /// Message propagation for p2p
    gossipsub: Gossipsub,

    /// RequestResponse protocol
    request_response: RequestResponse<MessageExchangeBincodeCodec>,

    /// Holds the Sender(s) part of the Oneshot Channel from the NetworkOrchestrator
    /// Once the ResponseMessage is received from the p2p Network
    /// It will send it to the NetworkOrchestrator via its unique Sender
    #[behaviour(ignore)]
    outbound_requests_table:
        HashMap<RequestId, oneshot::Sender<Result<ResponseMessage, ReqResNetworkError>>>,

    /// Holds the ResponseChannel(s) for the inbound requests from the p2p Network
    /// Once the ResponseMessage is prepared by the NetworkOrchestrator
    /// It will send it to the specified Peer via its unique ResponseChannel
    #[behaviour(ignore)]
    inbound_requests_table: HashMap<RequestId, ResponseChannel<ResponseMessage>>,

    /// Double-ended queue of FuelBehaviour Events
    #[behaviour(ignore)]
    events: VecDeque<FuelBehaviourEvent>,
}

impl FuelBehaviour {
    pub fn new(local_keypair: Keypair, p2p_config: &P2PConfig) -> Self {
        let local_public_key = local_keypair.public();
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

        let peer_info = PeerInfoBehaviour::new(local_public_key);

        let req_res_protocol =
            std::iter::once((MessageExchangeBincodeProtocol, ProtocolSupport::Full));

        let mut req_res_config = RequestResponseConfig::default();
        req_res_config
            .set_request_timeout(p2p_config.set_request_timeout.unwrap_or(REQ_RES_TIMEOUT));
        req_res_config.set_connection_keep_alive(
            p2p_config
                .set_connection_keep_alive
                .unwrap_or(REQ_RES_TIMEOUT),
        );

        let request_response = RequestResponse::new(
            MessageExchangeBincodeCodec {},
            req_res_protocol,
            req_res_config,
        );

        Self {
            discovery: discovery_config.finish(),
            gossipsub: gossipsub::build_gossipsub(&local_keypair, p2p_config),
            peer_info,
            request_response,

            outbound_requests_table: HashMap::default(),
            inbound_requests_table: HashMap::default(),
            events: VecDeque::default(),
        }
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peer_info.get_peer_info(peer_id)
    }

    pub fn get_peers(&self) -> &HashMap<PeerId, PeerInfo> {
        self.peer_info.peers()
    }

    pub fn publish_message(
        &mut self,
        topic: GossipTopic,
        data: impl Into<Vec<u8>>,
    ) -> Result<MessageId, PublishError> {
        self.gossipsub.publish(topic, data)
    }

    pub fn subscribe_to_topic(&mut self, topic: &GossipTopic) -> Result<bool, SubscriptionError> {
        self.gossipsub.subscribe(topic)
    }

    pub fn unsubscribe_from_topic(&mut self, topic: &GossipTopic) -> Result<bool, PublishError> {
        self.gossipsub.unsubscribe(topic)
    }

    pub fn send_request_msg(
        &mut self,
        message_request: RequestMessage,
        peer_id: PeerId,
        tx_channel: oneshot::Sender<Result<ResponseMessage, ReqResNetworkError>>,
    ) -> RequestId {
        let request_id = self
            .request_response
            .send_request(&peer_id, message_request);

        self.outbound_requests_table.insert(request_id, tx_channel);

        request_id
    }

    pub fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: ResponseMessage,
    ) -> Result<(), ResponseError> {
        if let Some(channel) = self.inbound_requests_table.remove(&request_id) {
            if let Err(e) = self.request_response.send_response(channel, message) {
                debug!(
                    "Failed to send ResponseMessage with error: {:?} for {:?}",
                    e, request_id
                );
                return Err(ResponseError::SendingResponseFailed);
            }
        } else {
            debug!("ResponseChannel for {:?} does not exist!", request_id);
            return Err(ResponseError::ResponseChannelDoesNotExist);
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
            <Self as NetworkBehaviour>::ProtocolsHandler,
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
    ) -> &HashMap<RequestId, oneshot::Sender<Result<ResponseMessage, ReqResNetworkError>>> {
        &self.outbound_requests_table
    }
}

impl NetworkBehaviourEventProcess<DiscoveryEvent> for FuelBehaviour {
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

impl NetworkBehaviourEventProcess<PeerInfoEvent> for FuelBehaviour {
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

impl NetworkBehaviourEventProcess<GossipsubEvent> for FuelBehaviour {
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message {
            propagation_source,
            message,
            message_id: _,
        } = message
        {
            self.events.push_back(FuelBehaviourEvent::GossipsubMessage {
                peer_id: propagation_source,
                topic_hash: message.topic,
                message: message.data,
            })
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<RequestMessage, ResponseMessage>>
    for FuelBehaviour
{
    fn inject_event(&mut self, event: RequestResponseEvent<RequestMessage, ResponseMessage>) {
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
                    if let Some(tx) = self.outbound_requests_table.remove(&request_id) {
                        if tx.send(Ok(response)).is_err() {
                            debug!("Failed to send through the channel for {:?}", request_id);
                        }
                    } else {
                        debug!("Send channel not found for {:?}", request_id);
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

                if let Some(tx) = self.outbound_requests_table.remove(&request_id) {
                    if tx.send(Err(error.into())).is_err() {
                        debug!("Failed to send through the channel for {:?}", request_id);
                    }
                }
            }
            _ => {}
        }
    }
}
