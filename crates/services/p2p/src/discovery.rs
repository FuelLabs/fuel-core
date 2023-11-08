use self::mdns::MdnsWrapper;
use futures::FutureExt;
use ip_network::IpNetwork;
use libp2p::{
    core::connection::ConnectionId,
    kad::{
        handler::KademliaHandlerProto,
        store::MemoryStore,
        Kademlia,
        QueryId,
    },
    mdns::Event as MdnsEvent,
    multiaddr::Protocol,
    swarm::{
        derive_prelude::{
            ConnectionClosed,
            ConnectionEstablished,
            FromSwarm,
        },
        ConnectionHandler,
        IntoConnectionHandler,
        NetworkBehaviour,
        NetworkBehaviourAction,
        PollParameters,
    },
    Multiaddr,
    PeerId,
};
use libp2p_kad::KademliaEvent;
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
mod mdns;
pub use discovery_config::DiscoveryConfig;

const SIXTY_SECONDS: Duration = Duration::from_secs(60);

/// NetworkBehavior for discovery of nodes
pub struct DiscoveryBehaviour {
    /// List of bootstrap nodes and their addresses
    bootstrap_nodes: Vec<(PeerId, Multiaddr)>,

    /// List of reserved nodes and their addresses
    reserved_nodes: Vec<(PeerId, Multiaddr)>,

    /// Track the connected peers
    connected_peers: HashSet<PeerId>,

    /// For discovery on local network, optionally available
    mdns: MdnsWrapper,

    /// Kademlia with MemoryStore
    kademlia: Kademlia<MemoryStore>,

    /// If enabled, the Stream that will fire after the delay expires,
    /// starting new random walk
    next_kad_random_walk: Option<Pin<Box<tokio::time::Sleep>>>,

    /// The Duration for the next random walk, after the current one ends
    duration_to_next_kad: Duration,

    /// Maximum amount of allowed peers
    max_peers_connected: usize,

    /// If false, `addresses_of_peer` won't return any private IPv4/IPv6 address,
    /// except for the ones stored in `bootstrap_nodes` and `reserved_peers`.
    allow_private_addresses: bool,
}

impl DiscoveryBehaviour {
    /// Adds a known listen address of a peer participating in the DHT to the routing table.
    pub fn add_address(&mut self, peer_id: &PeerId, address: Multiaddr) {
        self.kademlia.add_address(peer_id, address);
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ConnectionHandler = KademliaHandlerProto<QueryId>;
    type OutEvent = KademliaEvent;

    // Initializes new handler on a new opened connection
    fn new_handler(&mut self) -> Self::ConnectionHandler {
        // in our case we just return KademliaHandlerProto
        self.kademlia.new_handler()
    }

    // receive events from KademliaHandler and pass it down to kademlia
    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection: ConnectionId,
        event: <<Self::ConnectionHandler as IntoConnectionHandler>::Handler as ConnectionHandler>::OutEvent,
    ) {
        self.kademlia
            .on_connection_handler_event(peer_id, connection, event);
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
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
        self.kademlia.on_swarm_event(event)
    }

    // gets polled by the swarm
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        params: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
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
        if let Poll::Ready(kad_action) = self.kademlia.poll(cx, params) {
            return Poll::Ready(kad_action)
        };
        while let Poll::Ready(mdns_event) = self.mdns.poll(cx, params) {
            match mdns_event {
                NetworkBehaviourAction::GenerateEvent(MdnsEvent::Discovered(list)) => {
                    for (peer_id, multiaddr) in list {
                        self.kademlia.add_address(&peer_id, multiaddr);
                    }
                }
                NetworkBehaviourAction::ReportObservedAddr { address, score } => {
                    return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr {
                        address,
                        score,
                    })
                }
                NetworkBehaviourAction::CloseConnection {
                    peer_id,
                    connection,
                } => {
                    return Poll::Ready(NetworkBehaviourAction::CloseConnection {
                        peer_id,
                        connection,
                    })
                }
                _ => {}
            }
        }
        Poll::Pending
    }

    /// return list of known addresses for a given peer
    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        let mut list = self
            .bootstrap_nodes
            .iter()
            .chain(self.reserved_nodes.iter())
            .filter_map(|(current_peer_id, multiaddr)| {
                if current_peer_id == peer_id {
                    Some(multiaddr.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        {
            let mut list_to_filter = Vec::new();

            list_to_filter.extend(self.kademlia.addresses_of_peer(peer_id));
            list_to_filter.extend(self.mdns.addresses_of_peer(peer_id));

            // filter private addresses
            // nodes could potentially report addresses in the private network
            // which are not actually part of the network
            if !self.allow_private_addresses {
                list_to_filter.retain(|addr| match addr.iter().next() {
                    Some(Protocol::Ip4(addr)) if !IpNetwork::from(addr).is_global() => {
                        false
                    }
                    Some(Protocol::Ip6(addr)) if !IpNetwork::from(addr).is_global() => {
                        false
                    }
                    _ => true,
                });
            }

            list.extend(list_to_filter);
        }

        trace!("Addresses of {:?}: {:?}", peer_id, list);

        list
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveryBehaviour,
        DiscoveryConfig,
        KademliaEvent,
    };
    use futures::{
        future::poll_fn,
        StreamExt,
    };
    use libp2p::{
        core,
        identity::Keypair,
        multiaddr::Protocol,
        noise,
        swarm::{
            SwarmBuilder,
            SwarmEvent,
        },
        yamux,
        Multiaddr,
        PeerId,
        Swarm,
        Transport,
    };
    use std::{
        collections::{
            HashSet,
            VecDeque,
        },
        num::NonZeroU8,
        task::Poll,
        time::Duration,
    };

    /// helper function for building Discovery Behaviour for testing
    fn build_fuel_discovery(
        bootstrap_nodes: Vec<Multiaddr>,
    ) -> (Swarm<DiscoveryBehaviour>, Multiaddr, PeerId) {
        let keypair = Keypair::generate_secp256k1();
        let public_key = keypair.public();

        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&keypair)
            .unwrap();

        let transport = core::transport::MemoryTransport::new()
            .upgrade(core::upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed();

        let behaviour = {
            let mut config = DiscoveryConfig::new(
                keypair.public().to_peer_id(),
                "test_network".into(),
            );
            config
                .discovery_limit(50)
                .with_bootstrap_nodes(bootstrap_nodes)
                .set_connection_idle_timeout(Duration::from_secs(120))
                .with_random_walk(Duration::from_secs(5));

            config.finish()
        };

        let listen_addr: Multiaddr = Protocol::Memory(rand::random::<u64>()).into();
        let swarm_builder = SwarmBuilder::without_executor(
            transport,
            behaviour,
            keypair.public().to_peer_id(),
        )
        .dial_concurrency_factor(NonZeroU8::new(1).expect("1 > 0"));

        let mut swarm = swarm_builder.build();

        swarm
            .listen_on(listen_addr.clone())
            .expect("swarm should start listening");

        (swarm, listen_addr, PeerId::from_public_key(&public_key))
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

        let mut discovery_swarms = (0..num_of_swarms - 1)
            .map(|_| {
                build_fuel_discovery(vec![format!(
                    "{}/p2p/{}",
                    first_peer_addr.clone(),
                    first_peer_id
                )
                .parse()
                .unwrap()])
            })
            .collect::<VecDeque<_>>();

        discovery_swarms.push_front((first_swarm, first_peer_addr, first_peer_id));

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
                            SwarmEvent::Behaviour(KademliaEvent::UnroutablePeer {
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
