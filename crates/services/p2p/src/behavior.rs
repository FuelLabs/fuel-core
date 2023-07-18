use crate::{
    codecs::NetworkCodec,
    config::Config,
    discovery::{
        DiscoveryBehaviour,
        DiscoveryConfig,
        DiscoveryEvent,
    },
    gossipsub::{
        config::build_gossipsub_behaviour,
        topics::GossipTopic,
    },
    peer_report::{
        PeerReportBehaviour,
        PeerReportEvent,
    },
    request_response::messages::{
        NetworkResponse,
        RequestMessage,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use libp2p::{
    gossipsub::{
        error::PublishError,
        Gossipsub,
        GossipsubEvent,
        MessageAcceptance,
        MessageId,
    },
    request_response::{
        ProtocolSupport,
        RequestId,
        RequestResponse,
        RequestResponseConfig,
        RequestResponseEvent,
        ResponseChannel,
    },
    swarm::NetworkBehaviour,
    Multiaddr,
    PeerId,
};
use tracing::{
    debug,
    error,
    log::warn,
};

#[derive(Debug)]
pub enum FuelBehaviourEvent {
    Discovery(DiscoveryEvent),
    PeerReport(PeerReportEvent),
    Gossipsub(GossipsubEvent),
    RequestResponse(RequestResponseEvent<RequestMessage, NetworkResponse>),
}

/// Handles all p2p protocols needed for Fuel.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "FuelBehaviourEvent")]
pub struct FuelBehaviour<Codec: NetworkCodec> {
    /// Node discovery
    discovery: DiscoveryBehaviour,

    /// Identifies and periodically requests `BlockHeight` from connected nodes
    peer_report: PeerReportBehaviour,

    /// Message propagation for p2p
    gossipsub: Gossipsub,

    /// RequestResponse protocol
    request_response: RequestResponse<Codec>,
}

impl<Codec: NetworkCodec> FuelBehaviour<Codec> {
    pub(crate) fn new(p2p_config: &Config, codec: Codec) -> Self {
        let local_public_key = p2p_config.keypair.public();
        let local_peer_id = PeerId::from_public_key(&local_public_key);

        let discovery_config = {
            let mut discovery_config =
                DiscoveryConfig::new(local_peer_id, p2p_config.network_name.clone());

            discovery_config
                .enable_mdns(p2p_config.enable_mdns)
                .discovery_limit(p2p_config.max_peers_connected as usize)
                .allow_private_addresses(p2p_config.allow_private_addresses)
                .with_bootstrap_nodes(p2p_config.bootstrap_nodes.clone())
                .with_reserved_nodes(p2p_config.reserved_nodes.clone())
                .enable_reserved_nodes_only_mode(p2p_config.reserved_nodes_only_mode);

            if let Some(random_walk) = p2p_config.random_walk {
                discovery_config.with_random_walk(random_walk);
            }

            if let Some(duration) = p2p_config.connection_idle_timeout {
                discovery_config.set_connection_idle_timeout(duration);
            }

            discovery_config
        };

        let gossipsub = build_gossipsub_behaviour(p2p_config);

        let peer_report = PeerReportBehaviour::new(p2p_config);

        let req_res_protocol =
            std::iter::once((codec.get_req_res_protocol(), ProtocolSupport::Full));

        let mut req_res_config = RequestResponseConfig::default();
        req_res_config.set_request_timeout(p2p_config.set_request_timeout);
        req_res_config.set_connection_keep_alive(p2p_config.set_connection_keep_alive);

        let request_response =
            RequestResponse::new(codec, req_res_protocol, req_res_config);

        Self {
            discovery: discovery_config.finish(),
            gossipsub,
            peer_report,
            request_response,
        }
    }

    pub fn add_addresses_to_discovery(
        &mut self,
        peer_id: &PeerId,
        addresses: Vec<Multiaddr>,
    ) {
        for address in addresses {
            self.discovery.add_address(peer_id, address.clone());
        }
    }

    pub fn publish_message(
        &mut self,
        topic: GossipTopic,
        encoded_data: Vec<u8>,
    ) -> Result<MessageId, PublishError> {
        self.gossipsub.publish(topic, encoded_data)
    }

    pub fn send_request_msg(
        &mut self,
        message_request: RequestMessage,
        peer_id: &PeerId,
    ) -> RequestId {
        self.request_response.send_request(peer_id, message_request)
    }

    pub fn send_response_msg(
        &mut self,
        channel: ResponseChannel<NetworkResponse>,
        message: NetworkResponse,
    ) -> Result<(), NetworkResponse> {
        self.request_response.send_response(channel, message)
    }

    pub fn report_message_validation_result(
        &mut self,
        msg_id: &MessageId,
        propagation_source: &PeerId,
        acceptance: MessageAcceptance,
    ) -> Option<f64> {
        let should_check_score = matches!(acceptance, MessageAcceptance::Reject);

        match self.gossipsub.report_message_validation_result(
            msg_id,
            propagation_source,
            acceptance,
        ) {
            Ok(true) => {
                debug!(target: "fuel-p2p", "Sent a report for MessageId: {} from PeerId: {}", msg_id, propagation_source);
                if should_check_score {
                    return self.gossipsub.peer_score(propagation_source)
                }
            }
            Ok(false) => {
                warn!(target: "fuel-p2p", "Message with MessageId: {} not found in the Gossipsub Message Cache", msg_id);
            }
            Err(e) => {
                error!(target: "fuel-p2p", "Failed to report Message with MessageId: {} with Error: {:?}", msg_id, e);
            }
        }

        None
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.peer_report.update_block_height(block_height);
    }

    #[cfg(test)]
    pub fn get_peer_score(&self, peer_id: &PeerId) -> Option<f64> {
        self.gossipsub.peer_score(peer_id)
    }
}

impl From<DiscoveryEvent> for FuelBehaviourEvent {
    fn from(event: DiscoveryEvent) -> Self {
        FuelBehaviourEvent::Discovery(event)
    }
}

impl From<PeerReportEvent> for FuelBehaviourEvent {
    fn from(event: PeerReportEvent) -> Self {
        FuelBehaviourEvent::PeerReport(event)
    }
}

impl From<GossipsubEvent> for FuelBehaviourEvent {
    fn from(event: GossipsubEvent) -> Self {
        FuelBehaviourEvent::Gossipsub(event)
    }
}

impl From<RequestResponseEvent<RequestMessage, NetworkResponse>> for FuelBehaviourEvent {
    fn from(event: RequestResponseEvent<RequestMessage, NetworkResponse>) -> Self {
        FuelBehaviourEvent::RequestResponse(event)
    }
}
