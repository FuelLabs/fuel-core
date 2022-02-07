use crate::{
    config::P2PConfig,
    discovery::{DiscoveryBehaviour, DiscoveryConfig, DiscoveryEvent},
};
use libp2p::{
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
pub enum FuelBehaviourEvent {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
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

    #[behaviour(ignore)]
    events: VecDeque<FuelBehaviourEvent>,
}

impl FuelBehaviour {
    pub fn new(local_peer_id: PeerId, config: &P2PConfig) -> Self {
        let discovery_config = {
            let mut discovery_config =
                DiscoveryConfig::new(local_peer_id, config.network_name.clone());

            discovery_config
                .enable_mdns(config.enable_mdns)
                .discovery_limit(config.max_peers_connected)
                .allow_private_addresses(config.allow_private_addresses)
                .with_bootstrap_nodes(config.bootstrap_nodes.clone())
                .enable_random_walk(config.enable_random_walk);

            discovery_config
        };

        Self {
            discovery: discovery_config.finish(),
            events: VecDeque::default(),
        }
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
            DiscoveryEvent::Connected(peer_id) => self
                .events
                .push_back(FuelBehaviourEvent::PeerConnected(peer_id)),
            DiscoveryEvent::Disconnected(peer_id) => self
                .events
                .push_back(FuelBehaviourEvent::PeerDisconnected(peer_id)),
            _ => {}
        }
    }
}
