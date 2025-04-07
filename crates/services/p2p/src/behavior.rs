use crate::{
    codecs::{
        postcard::PostcardCodec,
        request_response::RequestResponseMessageHandler,
        RequestResponseProtocols,
    },
    config::Config,
    connection_limits,
    connection_limits::ConnectionLimits,
    discovery,
    gossipsub::config::build_gossipsub_behaviour,
    heartbeat,
    limited_behaviour::LimitedBehaviour,
    peer_report,
    request_response::messages::{
        RequestMessage,
        V2ResponseMessage,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use libp2p::{
    allow_block_list,
    gossipsub::{
        self,
        MessageAcceptance,
        MessageId,
        PublishError,
        TopicHash,
    },
    identify,
    request_response::{
        self,
        OutboundRequestId,
        ProtocolSupport,
        ResponseChannel,
    },
    swarm::NetworkBehaviour,
    Multiaddr,
    PeerId,
};
use std::collections::HashSet;

const MAX_PENDING_INCOMING_CONNECTIONS: u32 = 100;
const MAX_PENDING_OUTGOING_CONNECTIONS: u32 = 100;

/// Handles all p2p protocols needed for Fuel.
#[derive(NetworkBehaviour)]
pub struct FuelBehaviour {
    /// **WARNING**: The order of the behaviours is important and fragile, at least for the tests.
    /// The Behaviour to manage connections to blocked peers.
    blocked_peer: allow_block_list::Behaviour<allow_block_list::BlockedPeers>,

    /// The Behaviour to manage connection limits.
    connection_limits: connection_limits::Behaviour,

    /// Node discovery
    discovery: discovery::Behaviour,

    /// Message propagation for p2p
    gossipsub: LimitedBehaviour<gossipsub::Behaviour>,

    /// Handles regular heartbeats from peers
    heartbeat: LimitedBehaviour<heartbeat::Behaviour>,

    /// The Behaviour to identify peers.
    identify: LimitedBehaviour<identify::Behaviour>,

    /// Identifies and periodically requests `BlockHeight` from connected nodes
    peer_report: LimitedBehaviour<peer_report::Behaviour>,

    /// RequestResponse protocol
    request_response: LimitedBehaviour<
        request_response::Behaviour<RequestResponseMessageHandler<PostcardCodec>>,
    >,
}

impl FuelBehaviour {
    pub(crate) fn new(
        p2p_config: &Config,
        request_response_codec: RequestResponseMessageHandler<PostcardCodec>,
    ) -> anyhow::Result<Self> {
        let local_public_key = p2p_config.keypair.public();
        let local_peer_id = PeerId::from_public_key(&local_public_key);

        let discovery_config = {
            let mut discovery_config =
                discovery::Config::new(local_peer_id, p2p_config.network_name.clone());

            discovery_config
                .enable_mdns(p2p_config.enable_mdns)
                .max_peers_connected(p2p_config.max_discovery_peers_connected as usize)
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

        let peer_report = peer_report::Behaviour::new(&p2p_config.reserved_nodes);
        let reserved_peer_ids: HashSet<_> = peer_report
            .reserved_nodes_multiaddr()
            .keys()
            .cloned()
            .collect();

        let identify = {
            let identify_config = identify::Config::new(
                "/fuel/1.0".to_string(),
                p2p_config.keypair.public(),
            );
            if let Some(interval) = p2p_config.identify_interval {
                identify::Behaviour::new(identify_config.with_interval(interval))
            } else {
                identify::Behaviour::new(identify_config)
            }
        };

        let heartbeat = heartbeat::Behaviour::new(
            p2p_config.heartbeat_config.clone(),
            BlockHeight::default(),
        );

        let connection_limits = connection_limits::Behaviour::new(
            ConnectionLimits::default()
                .with_max_pending_incoming(Some(MAX_PENDING_INCOMING_CONNECTIONS))
                .with_max_pending_outgoing(Some(
                    p2p_config
                        .max_outgoing_connections
                        .min(MAX_PENDING_OUTGOING_CONNECTIONS),
                ))
                .with_max_established_outgoing(Some(p2p_config.max_outgoing_connections))
                .with_max_established_per_peer(p2p_config.max_connections_per_peer)
                .with_max_established(Some(p2p_config.max_discovery_peers_connected)),
            reserved_peer_ids,
        );
        let connections = connection_limits.connections();

        let req_res_protocol = request_response_codec
            .get_req_res_protocols()
            .map(|protocol| (protocol, ProtocolSupport::Full));

        let req_res_config = request_response::Config::default()
            .with_request_timeout(p2p_config.set_request_timeout)
            .with_max_concurrent_streams(p2p_config.max_concurrent_streams);

        let request_response = request_response::Behaviour::with_codec(
            request_response_codec.clone(),
            req_res_protocol,
            req_res_config,
        );

        let discovery = discovery_config.finish()?;
        let limit = p2p_config.max_functional_peers_connected as usize;

        let gossipsub = LimitedBehaviour::new(limit, connections.clone(), gossipsub);
        let peer_report = LimitedBehaviour::new(limit, connections.clone(), peer_report);
        let identify = LimitedBehaviour::new(limit, connections.clone(), identify);
        let heartbeat = LimitedBehaviour::new(limit, connections.clone(), heartbeat);
        let request_response =
            LimitedBehaviour::new(limit, connections.clone(), request_response);

        Ok(Self {
            discovery,
            gossipsub,
            peer_report,
            request_response,
            blocked_peer: Default::default(),
            identify,
            heartbeat,
            connection_limits,
        })
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
        topic_hash: TopicHash,
        encoded_data: Vec<u8>,
    ) -> Result<MessageId, PublishError> {
        self.gossipsub.publish(topic_hash, encoded_data)
    }

    pub fn send_request_msg(
        &mut self,
        message_request: RequestMessage,
        peer_id: &PeerId,
    ) -> OutboundRequestId {
        self.request_response.send_request(peer_id, message_request)
    }

    pub fn send_response_msg(
        &mut self,
        channel: ResponseChannel<V2ResponseMessage>,
        message: V2ResponseMessage,
    ) -> Result<(), V2ResponseMessage> {
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
                tracing::debug!(target: "fuel-p2p", "Sent a report for MessageId: {} from PeerId: {}", msg_id, propagation_source);
                if should_check_score {
                    return self.gossipsub.peer_score(propagation_source);
                }
            }
            Ok(false) => {
                tracing::warn!(target: "fuel-p2p", "Message with MessageId: {} not found in the Gossipsub Message Cache", msg_id);
            }
            Err(e) => {
                tracing::error!(target: "fuel-p2p", "Failed to report Message with MessageId: {} with Error: {:?}", msg_id, e);
            }
        }

        None
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.heartbeat.update_block_height(block_height);
    }

    #[cfg(test)]
    pub fn get_peer_score(&self, peer_id: &PeerId) -> Option<f64> {
        self.gossipsub.peer_score(peer_id)
    }

    pub fn block_peer(&mut self, peer_id: PeerId) {
        self.blocked_peer.block_peer(peer_id)
    }
}
