use crate::{
    config::P2PConfig,
    discovery::{DiscoveryBehaviour, DiscoveryConfig, DiscoveryEvent},
    peer_info::{PeerInfo, PeerInfoBehaviour, PeerInfoEvent},
};
use libp2p::{
    identity::Keypair,
    swarm::{
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
    },
    NetworkBehaviour, PeerId,
};
use std::{
    collections::VecDeque,
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

    #[behaviour(ignore)]
    events: VecDeque<FuelBehaviourEvent>,
}

impl FuelBehaviour {
    pub fn new(local_keypair: Keypair, config: &P2PConfig) -> Self {
        let local_public_key = local_keypair.public();
        let local_peer_id = PeerId::from_public_key(&local_public_key);

        let discovery_config = {
            let mut discovery_config =
                DiscoveryConfig::new(local_peer_id, config.network_name.clone());

            discovery_config
                .enable_mdns(config.enable_mdns)
                .discovery_limit(config.max_peers_connected)
                .allow_private_addresses(config.allow_private_addresses)
                .with_bootstrap_nodes(config.bootstrap_nodes.clone())
                .enable_random_walk(config.enable_random_walk);

            if let Some(duration) = config.connection_idle_timeout {
                discovery_config.set_connection_idle_timeout(duration);
            }

            discovery_config
        };

        let peer_info = PeerInfoBehaviour::new(local_public_key);

        Self {
            discovery: discovery_config.finish(),
            peer_info,
            events: VecDeque::default(),
        }
    }

    #[allow(dead_code)]
    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peer_info.get_peer_info(peer_id)
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
