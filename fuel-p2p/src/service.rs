use crate::{
    behavior::{FuelBehaviour, FuelBehaviourEvent},
    config::P2PConfig,
    utils::load_private_key,
};
use futures::prelude::*;
use libp2p::{multiaddr::Protocol, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use log::{info, warn};
use std::error::Error;

pub struct FuelP2PService {
    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour>,
    /// This node's PeerId.
    pub local_peer_id: PeerId,
}

pub enum FuelP2PEvent {
    Behaviour(FuelBehaviourEvent),
}

impl FuelP2PService {
    pub async fn new(config: P2PConfig) -> Result<Self, Box<dyn Error>> {
        // get local Keypair
        let local_key = load_private_key(&config);
        let peer_id = PeerId::from(local_key.public());

        // configure and build P2P Serivce
        let transport = libp2p::development_transport(local_key.clone()).await?;
        let behaviour = FuelBehaviour::new(local_key);
        let mut swarm = Swarm::new(transport, behaviour, peer_id);

        // set up node's address to listen on
        let listen_multiaddr = {
            let mut m = Multiaddr::from(config.address);
            m.push(Protocol::Tcp(config.tcp_port));
            m
        };

        // start listening at the given address
        swarm.listen_on(listen_multiaddr)?;

        // connect to config-provided peers
        for peer_addr in config.peers {
            match swarm.dial(peer_addr.clone()) {
                Ok(_) => info!("connected to a peer: {}", peer_addr),
                Err(error) => warn!("dialing peer {} failed with {:?}", peer_addr, error),
            }
        }

        Ok(Self {
            swarm,
            local_peer_id: peer_id,
        })
    }

    pub async fn next_event(&mut self) -> FuelP2PEvent {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::Behaviour(fuel_behaviour) => {
                    return FuelP2PEvent::Behaviour(fuel_behaviour)
                }
                _ => {
                    // todo: handle other necessary events
                }
            }
        }
    }
}
