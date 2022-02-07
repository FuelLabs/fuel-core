use crate::{
    behavior::{FuelBehaviour, FuelBehaviourEvent},
    config::P2PConfig,
};
use futures::prelude::*;
use libp2p::{identity::Keypair, multiaddr::Protocol, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use std::error::Error;

/// Listens to evets on the p2p network
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
        let behaviour = FuelBehaviour::new(local_peer_id, &config);
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
    use crate::{config::P2PConfig, service::FuelP2PEvent};
    use libp2p::identity::Keypair;
    use std::net::{IpAddr, Ipv4Addr};

    /// helper function for building default testing config
    fn build_p2p_config() -> P2PConfig {
        P2PConfig {
            network_name: "test_network".into(),
            address: IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0])),
            tcp_port: 4000,
            bootstrap_nodes: vec![],
            enable_mdns: true,
            max_peers_connected: 50,
            allow_private_addresses: true,
            enable_random_walk: true,
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

    #[tokio::test]
    async fn two_services_connected() {
        // First P2P Service
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4001;
        let mut first_p2p_service = build_fuel_p2p_service(p2p_config).await;

        let first_p2p_service_address = match first_p2p_service.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Second P2P Service
        let mut p2p_config = build_p2p_config();
        p2p_config.tcp_port = 4002;
        p2p_config.bootstrap_nodes = vec![(
            first_p2p_service.local_peer_id,
            first_p2p_service_address.unwrap(),
        )];
        let mut second_p2p_service = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                p2p_event_on_second_service = second_p2p_service.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerConnected(_)) =  p2p_event_on_second_service {
                        // successfully connected to the first service
                        break
                    }
                },
                _ = first_p2p_service.next_event() => {}
            };
        }
    }
}
