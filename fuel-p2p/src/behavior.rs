use crate::{
    config::P2PConfig,
    discovery::{DiscoveryBehaviour, DiscoveryConfig, DiscoveryEvent},
    gossipsub,
    peer_info::{PeerInfo, PeerInfoBehaviour, PeerInfoEvent},
    service::GossipTopic,
};
use libp2p::{
    gossipsub::{
        error::{PublishError, SubscriptionError},
        Gossipsub, GossipsubEvent, MessageId, TopicHash,
    },
    identity::Keypair,
    swarm::{
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
    },
    NetworkBehaviour, PeerId,
};
use std::{
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
};

// todo: define which events outside world is intersted in
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
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

    /// message propagation for p2p
    gossipsub: Gossipsub,

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

        Self {
            discovery: discovery_config.finish(),
            gossipsub: gossipsub::build_gossipsub(&local_keypair, p2p_config),
            peer_info,
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
