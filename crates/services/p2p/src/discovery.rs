use self::mdns_wrapper::MdnsWrapper;
use futures::FutureExt;
use libp2p::{
    core::Endpoint,
    kad::{
        self,
        store::MemoryStore,
    },
    mdns,
    swarm::{
        derive_prelude::{
            ConnectionClosed,
            ConnectionEstablished,
            FromSwarm,
        },
        ConnectionDenied,
        ConnectionId,
        NetworkBehaviour,
        THandler,
    },
    Multiaddr,
    PeerId,
};

use libp2p::swarm::{
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};
use std::{
    collections::HashSet,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};
use tracing::trace;
mod discovery_config;
mod mdns_wrapper;
pub use discovery_config::Config;

const SIXTY_SECONDS: Duration = Duration::from_secs(60);

pub type Event = kad::Event;

/// NetworkBehavior for discovery of nodes
pub struct Behaviour {
    /// Track the connected peers
    connected_peers: HashSet<PeerId>,

    /// For discovery on local network, optionally available
    mdns: MdnsWrapper,

    /// Kademlia with MemoryStore
    kademlia: kad::Behaviour<MemoryStore>,

    /// If enabled, the Stream that will fire after the delay expires,
    /// starting new random walk
    next_kad_random_walk: Option<Pin<Box<tokio::time::Sleep>>>,

    /// The Duration for the next random walk, after the current one ends
    duration_to_next_kad: Duration,

    /// Maximum amount of allowed peers
    max_peers_connected: usize,
}

impl Behaviour {
    /// Adds a known listen address of a peer participating in the DHT to the routing table.
    pub fn add_address(&mut self, peer_id: &PeerId, address: Multiaddr) {
        self.kademlia.add_address(peer_id, address);
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        <kad::Behaviour<MemoryStore> as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = kad::Event;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.kademlia.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    // receive events from KademliaHandler and pass it down to kademlia
    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut kademlia_addrs = self.kademlia.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;
        let mdns_addrs = self.mdns.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;
        kademlia_addrs.extend(mdns_addrs);
        Ok(kademlia_addrs)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.kademlia.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match &event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                other_established,
                ..
            }) => {
                if *other_established == 0 {
                    self.connected_peers.insert(*peer_id);

                    trace!("Connected to a peer {:?}", peer_id);
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) => {
                if *remaining_established == 0 {
                    self.connected_peers.remove(peer_id);
                    trace!("Disconnected from {:?}", peer_id);
                }
            }
            _ => (),
        }
        self.mdns.on_swarm_event(&event);
        self.kademlia.on_swarm_event(event);
    }

    // receive events from KademliaHandler and pass it down to kademlia
    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.kademlia
            .on_connection_handler_event(peer_id, connection, event);
    }

    // gets polled by the swarm
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // if random walk is enabled poll the stream that will fire when random walk is scheduled
        if let Some(next_kad_random_query) = self.next_kad_random_walk.as_mut() {
            while next_kad_random_query.poll_unpin(cx).is_ready() {
                if self.connected_peers.len() < self.max_peers_connected {
                    let random_peer_id = PeerId::random();
                    self.kademlia.get_closest_peers(random_peer_id);
                }

                *next_kad_random_query =
                    Box::pin(tokio::time::sleep(self.duration_to_next_kad));
                // duration to next random walk should either be exponentially bigger than the previous
                // or at max 60 seconds
                self.duration_to_next_kad = std::cmp::min(
                    self.duration_to_next_kad.saturating_mul(2),
                    SIXTY_SECONDS,
                );
            }
        }

        // poll sub-behaviors
        if let Poll::Ready(kad_action) = self.kademlia.poll(cx) {
            return Poll::Ready(kad_action)
        };

        while let Poll::Ready(mdns_event) = self.mdns.poll(cx) {
            match mdns_event {
                ToSwarm::GenerateEvent(mdns::Event::Discovered(list)) => {
                    for (peer_id, multiaddr) in list {
                        self.kademlia.add_address(&peer_id, multiaddr);
                    }
                }
                ToSwarm::CloseConnection {
                    peer_id,
                    connection,
                } => {
                    return Poll::Ready(ToSwarm::CloseConnection {
                        peer_id,
                        connection,
                    })
                }
                _ => {}
            }
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Behaviour,
        Config,
        Event,
    };
    use futures::{
        future::poll_fn,
        StreamExt,
    };
    use libp2p::{
        identity::Keypair,
        multiaddr::Protocol,
        swarm::SwarmEvent,
        Multiaddr,
        PeerId,
        Swarm,
    };
    use std::{
        collections::HashSet,
        task::Poll,
        time::Duration,
    };

    use libp2p_swarm_test::SwarmExt;

    const MAX_PEERS: usize = 50;

    fn build_behavior_fn(
        bootstrap_nodes: Vec<Multiaddr>,
    ) -> impl FnOnce(Keypair) -> Behaviour {
        |keypair| {
            let mut config =
                Config::new(keypair.public().to_peer_id(), "test_network".into());
            config
                .max_peers_connected(MAX_PEERS)
                .with_bootstrap_nodes(bootstrap_nodes)
                .with_random_walk(Duration::from_millis(500));

            config.finish()
        }
    }

    /// helper function for building Discovery Behaviour for testing
    fn build_fuel_discovery(
        bootstrap_nodes: Vec<Multiaddr>,
    ) -> (Swarm<Behaviour>, Multiaddr, PeerId) {
        let behaviour_fn = build_behavior_fn(bootstrap_nodes);

        let listen_addr: Multiaddr = Protocol::Memory(rand::random::<u64>()).into();

        let mut swarm = Swarm::new_ephemeral(behaviour_fn);

        swarm
            .listen_on(listen_addr.clone())
            .expect("swarm should start listening");

        let peer_id = swarm.local_peer_id().to_owned();

        (swarm, listen_addr, peer_id)
    }

    // builds 25 discovery swarms,
    // initially, only connects first_swarm to the rest of the swarms
    // after that each swarm uses kademlia to discover other swarms
    // test completes after all swarms have connected to each other
    #[tokio::test]
    async fn discovery_works() {
        // Number of peers in the network
        let num_of_swarms = 25;
        let (first_swarm, first_peer_addr, first_peer_id) = build_fuel_discovery(vec![]);
        let bootstrap_addr: Multiaddr =
            format!("{}/p2p/{}", first_peer_addr.clone(), first_peer_id)
                .parse()
                .unwrap();

        let mut discovery_swarms = Vec::new();
        discovery_swarms.push((first_swarm, first_peer_addr, first_peer_id));

        for _ in 1..num_of_swarms {
            let (swarm, peer_addr, peer_id) =
                build_fuel_discovery(vec![bootstrap_addr.clone()]);

            discovery_swarms.push((swarm, peer_addr, peer_id));
        }

        // HashSet of swarms to discover for each swarm
        let mut left_to_discover = (0..discovery_swarms.len())
                .map(|current_index| {
                    (0..discovery_swarms.len())
                        .skip(1) // first_swarm is already connected
                        .filter_map(|swarm_index| {
                            // filter your self
                            if swarm_index != current_index {
                                // get the PeerId
                                Some(*Swarm::local_peer_id(&discovery_swarms[swarm_index].0))
                            } else {
                                None
                            }
                        })
                        .collect::<HashSet<_>>()
                })
                .collect::<Vec<_>>();

        let test_future = poll_fn(move |cx| {
            'polling: loop {
                for swarm_index in 0..discovery_swarms.len() {
                    if let Poll::Ready(Some(event)) =
                        discovery_swarms[swarm_index].0.poll_next_unpin(cx)
                    {
                        match event {
                            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                // if peer has connected - remove it from the set
                                left_to_discover[swarm_index].remove(&peer_id);
                            }
                            SwarmEvent::Behaviour(Event::UnroutablePeer {
                                peer: peer_id,
                            }) => {
                                // kademlia discovered a peer but does not have it's address
                                // we simulate Identify happening and provide the address
                                let unroutable_peer_addr = discovery_swarms
                                    .iter()
                                    .find_map(|(_, next_addr, next_peer_id)| {
                                        // identify the peer
                                        if next_peer_id == &peer_id {
                                            // and return it's address
                                            Some(next_addr.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap();

                                // kademlia must be informed of a peer's address before
                                // adding it to the routing table
                                discovery_swarms[swarm_index]
                                    .0
                                    .behaviour_mut()
                                    .kademlia
                                    .add_address(&peer_id, unroutable_peer_addr.clone());
                            }
                            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                                panic!("PeerId {peer_id:?} disconnected");
                            }
                            _ => {}
                        }
                        continue 'polling
                    }
                }
                break
            }

            // if there are no swarms left to discover we are done with the discovery
            if left_to_discover.iter().all(|l| l.is_empty()) {
                // we are done!
                Poll::Ready(())
            } else {
                // keep polling Discovery Behaviour
                Poll::Pending
            }
        });

        test_future.await;
    }
}
