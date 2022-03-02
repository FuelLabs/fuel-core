use crate::{
    behavior::{FuelBehaviour, FuelBehaviourEvent},
    config::P2PConfig,
};
use futures::prelude::*;
use libp2p::{identity::Keypair, multiaddr::Protocol, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use std::error::Error;

/// Listens to the events on the p2p network
/// And forwards them to the Orchestrator
pub struct FuelP2PService {
    /// Store the local peer id
    pub local_peer_id: PeerId,
    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour>,
}

// todo: add other network events
#[derive(Debug)]
pub enum FuelP2PEvent {
    Behaviour(FuelBehaviourEvent),
    NewListenAddr(Multiaddr),
}

impl FuelP2PService {
    pub async fn new(local_keypair: Keypair, config: P2PConfig) -> Result<Self, Box<dyn Error>> {
        let local_peer_id = PeerId::from(local_keypair.public());

        // configure and build P2P Serivce
        let transport = libp2p::development_transport(local_keypair.clone()).await?;
        let behaviour = FuelBehaviour::new(local_keypair, &config);
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);

        // set up node's address to listen on
        let listen_multiaddr = {
            let mut m = Multiaddr::from(config.address);
            m.push(Protocol::Tcp(config.tcp_port));
            m
        };

        // start listening at the given address
        swarm.listen_on(listen_multiaddr)?;

        Ok(Self {
            swarm,
            local_peer_id,
        })
    }

    pub async fn next_event(&mut self) -> FuelP2PEvent {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::Behaviour(fuel_behaviour) => {
                    return FuelP2PEvent::Behaviour(fuel_behaviour)
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    return FuelP2PEvent::NewListenAddr(address)
                }
                _ => {
                    // todo: handle other necessary events
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FuelBehaviourEvent, FuelP2PService};
    use crate::{config::P2PConfig, peer_info::PeerInfo, service::FuelP2PEvent};
    use libp2p::identity::Keypair;
    use std::{
        net::{IpAddr, Ipv4Addr},
        time::Duration,
    };

    /// helper function for building default testing config
    fn build_p2p_config() -> P2PConfig {
        P2PConfig {
            network_name: "test_network".into(),
            address: IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0])),
            tcp_port: 4000,
            bootstrap_nodes: vec![],
            enable_mdns: false,
            max_peers_connected: 50,
            allow_private_addresses: true,
            enable_random_walk: true,
            connection_idle_timeout: Some(Duration::from_secs(120)),
        }
    }

    /// helper function for building FuelP2PService
    async fn build_fuel_p2p_service(p2p_config: P2PConfig) -> FuelP2PService {
        let keypair = Keypair::generate_secp256k1();
        let fuel_p2p_service = FuelP2PService::new(keypair, p2p_config).await.unwrap();

        fuel_p2p_service
    }

    #[tokio::test]
    async fn p2p_service_works() {
        let mut fuel_p2p_service = build_fuel_p2p_service(build_p2p_config()).await;

        loop {
            match fuel_p2p_service.next_event().await {
                FuelP2PEvent::NewListenAddr(_address) => {
                    // listener address registered, we are good to go
                    break;
                }
                other_event => {
                    panic!("Unexpected event: {:?}", other_event)
                }
            }
        }
    }

    // Simulates 2 p2p nodes that are on the same network and should connect via mDNS
    // without any additional bootstrapping
    #[tokio::test]
    async fn nodes_connected_via_mdns() {
        // Node A
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4001;
        p2p_config.enable_mdns = true;
        let mut node_a = build_fuel_p2p_service(p2p_config).await;

        // Node B
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4002;
        p2p_config.enable_mdns = true;
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                p2p_event_on_second_service = node_b.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerConnected(_)) = p2p_event_on_second_service {
                        // successfully connected to Node B
                        break
                    }
                },
                _ = node_a.next_event() => {}
            };
        }
    }

    // Simulates 3 p2p nodes, Node B & Node C are bootstrapped with Node A
    // Using Identify Protocol Node C should be able to identify and connect to Node B
    #[tokio::test]
    async fn nodes_connected_via_identify() {
        // Node A
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4003;
        let mut node_a = build_fuel_p2p_service(p2p_config).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4004;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        // Node C
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4005;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.unwrap())];
        let mut node_c = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                _ = node_a.next_event() => {},
                _ = node_b.next_event() => {},

                node_c_p2p_event = node_c.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerConnected(peer_id)) = node_c_p2p_event {
                        // we have connected to Node B!
                        if peer_id == node_b.local_peer_id {
                            break
                        }
                    }
                }
            };
        }
    }

    // Simulates 2 p2p nodes that connect to each other and consequently exchange Peer Info
    #[tokio::test]
    async fn peer_info_updates_work() {
        // Node A
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4006;
        let mut node_a = build_fuel_p2p_service(p2p_config).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4007;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                event_a = node_a.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerInfoUpdated(peer_id)) = event_a {
                        let PeerInfo { peer_addresses, latest_ping, client_version, .. } = node_a.swarm.behaviour().get_peer_info(&peer_id).unwrap();

                        // Exits after it verifies that:
                        // 1. Peer Addresses are known
                        // 2. Client Version is known
                        // 3. Node has been pinged and responded with success
                        if !peer_addresses.is_empty() && client_version.is_some() && latest_ping.is_some() {
                            break;
                        }
                    }

                },
                _ = node_b.next_event() => {}
            };
        }
    }
}
