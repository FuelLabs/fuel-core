use crate::config::Config;
use libp2p::{
    core::{
        connection::ConnectionId,
        either::EitherOutput,
    },
    identify::{
        Behaviour as Identify,
        Config as IdentifyConfig,
        Event as IdentifyEvent,
        Info as IdentifyInfo,
    },
    ping::{
        Behaviour as Ping,
        Config as PingConfig,
        Event as PingEvent,
        Success as PingSuccess,
    },
    swarm::{
        ConnectionHandler,
        IntoConnectionHandler,
        IntoConnectionHandlerSelect,
        NetworkBehaviour,
        NetworkBehaviourAction,
        PollParameters,
    },
    Multiaddr,
    PeerId,
};
use libp2p_swarm::derive_prelude::{
    ConnectionClosed,
    ConnectionEstablished,
    DialFailure,
    FromSwarm,
    ListenFailure,
};
use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    task::{
        Context,
        Poll,
    },
    time::Duration,
};
use tracing::debug;

/// Maximum amount of peer's addresses that we are ready to store per peer
const MAX_IDENTIFY_ADDRESSES: usize = 10;

/// Events emitted by PeerInfoBehaviour
#[derive(Debug)]
pub enum PeerInfoEvent {
    PeerConnected(PeerId),
    PeerDisconnected {
        peer_id: PeerId,
        should_reconnect: bool,
    },
    TooManyPeers {
        peer_to_disconnect: PeerId,
        peer_to_connect: Option<PeerId>,
    },
    PeerIdentified {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    PeerInfoUpdated {
        peer_id: PeerId,
    },
}

#[derive(Debug, Clone)]
pub struct PeerInfo<'a> {
    pub peer_addresses: &'a HashSet<Multiaddr>,
    pub client_version: &'a Option<String>,
    pub latest_ping: &'a Option<Duration>,
}

impl<'a> From<&'a ConnectedPeerInfo> for PeerInfo<'a> {
    fn from(value: &'a ConnectedPeerInfo) -> Self {
        PeerInfo {
            peer_addresses: &value.peer_addresses,
            client_version: &value.client_version,
            latest_ping: &value.latest_ping,
        }
    }
}

impl<'a> From<&'a ReservedPeerInfo> for PeerInfo<'a> {
    fn from(value: &'a ReservedPeerInfo) -> Self {
        PeerInfo {
            peer_addresses: &value.peer_addresses,
            client_version: &value.client_version,
            latest_ping: &value.latest_ping,
        }
    }
}

// Info about a single Peer that we're connected to
#[derive(Debug, Default, Clone)]
pub struct ConnectedPeerInfo {
    pub peer_addresses: HashSet<Multiaddr>,
    pub client_version: Option<String>,
    pub latest_ping: Option<Duration>,
}

#[derive(Debug, Default, Clone)]
struct ReservedPeerInfo {
    peer_addresses: HashSet<Multiaddr>,
    client_version: Option<String>,
    latest_ping: Option<Duration>,
    is_connected: bool,
}

// `Behaviour` that holds info about peers
pub struct PeerInfoBehaviour {
    ping: Ping,
    identify: Identify,
    pending_events: VecDeque<PeerInfoEvent>,
    connected_peers: HashMap<PeerId, ConnectedPeerInfo>,
    reserved_peers: HashMap<PeerId, ReservedPeerInfo>,
}

impl PeerInfoBehaviour {
    pub fn new(config: &Config) -> Self {
        let identify = {
            let identify_config =
                IdentifyConfig::new("/fuel/1.0".to_string(), config.keypair.public());
            if let Some(interval) = config.identify_interval {
                Identify::new(identify_config.with_interval(interval))
            } else {
                Identify::new(identify_config)
            }
        };

        let ping = {
            let ping_config = PingConfig::new();
            if let Some(interval) = config.info_interval {
                Ping::new(ping_config.with_interval(interval))
            } else {
                Ping::new(ping_config)
            }
        };

        let reserved_peers: HashMap<PeerId, ReservedPeerInfo> = config
            .reserved_nodes
            .iter()
            .filter_map(|addr| {
                Some((
                    PeerId::try_from_multiaddr(addr)?,
                    ReservedPeerInfo::default(),
                ))
            })
            .collect();

        let non_reserved_peers_capacity =
            config.max_peers_connected as usize - reserved_peers.len();

        Self {
            ping,
            identify,
            pending_events: VecDeque::default(),
            connected_peers: HashMap::with_capacity(non_reserved_peers_capacity),
            reserved_peers,
        }
    }

    pub fn get_peers_ids(&self) -> Vec<&PeerId> {
        let connected_peers = self.connected_peers.keys();
        let reserved_peers = self.reserved_peers.keys();
        connected_peers.chain(reserved_peers).collect()
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.connected_peers
            .get(peer_id)
            .map(Into::into)
            .or_else(|| self.reserved_peers.get(peer_id).map(Into::into))
    }

    pub fn insert_peer_addresses(&mut self, peer_id: &PeerId, addresses: Vec<Multiaddr>) {
        if let Some(reserved_peer) = self.reserved_peers.get_mut(peer_id) {
            for address in addresses {
                reserved_peer.peer_addresses.insert(address);
            }
        } else if let Some(peer) = self.connected_peers.get_mut(peer_id) {
            for address in addresses {
                peer.peer_addresses.insert(address);
            }
        } else {
            log_missing_peer(peer_id);
        }
    }

    /// Insert latest ping to a connected Node
    fn insert_latest_ping(&mut self, peer_id: &PeerId, duration: Duration) {
        if let Some(reserved_peer) = self.reserved_peers.get_mut(peer_id) {
            reserved_peer.latest_ping = Some(duration);
        } else if let Some(peer) = self.connected_peers.get_mut(peer_id) {
            peer.latest_ping = Some(duration);
        } else {
            log_missing_peer(peer_id);
        }
    }

    fn insert_client_version(&mut self, peer_id: &PeerId, client_version: String) {
        if let Some(reserved_peer) = self.reserved_peers.get_mut(peer_id) {
            reserved_peer.client_version = Some(client_version);
        } else if let Some(peer) = self.connected_peers.get_mut(peer_id) {
            peer.client_version = Some(client_version);
        } else {
            log_missing_peer(peer_id);
        }
    }

    /// Handles the first connnection established with a Peer
    fn handle_initial_connection(&mut self, peer_id: PeerId) {
        if let Some(reserved_peer) = self.reserved_peers.get_mut(&peer_id) {
            // Reserved Peers are never removed, just their connection is tracked
            reserved_peer.is_connected = true;
        } else if self.connected_peers.len() == self.connected_peers.capacity() {
            // we are at max capacity of non-reserved peers allowed
            // disconnect the newly connected peer
            // this should only happen when one of our reserved nodes is not currently connected
            // so there is some extra space for connections

            let disconnected_reserved_peeer = self
                .reserved_peers
                .iter()
                .find(|(_, peer_info)| !peer_info.is_connected)
                .map(|(peer_id, _)| *peer_id);

            // todo/potential improvement: once `Peer Reputation` is implemented we could check if there are peers
            // with poor reputation and disconnect them instead?
            self.pending_events.push_back(PeerInfoEvent::TooManyPeers {
                peer_to_disconnect: peer_id,
                peer_to_connect: disconnected_reserved_peeer,
            });

            // early return, no need to report on newly established connection
            // since the peer will be disconnected
            return
        } else {
            self.connected_peers
                .insert(peer_id, ConnectedPeerInfo::default());
        }

        self.pending_events
            .push_back(PeerInfoEvent::PeerConnected(peer_id));
    }

    fn handle_last_connection(&mut self, peer_id: PeerId) {
        let should_reconnect =
            if let Some(reserved_peer) = self.reserved_peers.get_mut(&peer_id) {
                reserved_peer.is_connected = false;
                // we need to connect to this peer again
                true
            } else {
                self.connected_peers.remove(&peer_id);
                false
            };

        self.pending_events
            .push_back(PeerInfoEvent::PeerDisconnected {
                peer_id,
                should_reconnect,
            })
    }
}

fn log_missing_peer(peer_id: &PeerId) {
    debug!(target: "fuel-libp2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
}

impl NetworkBehaviour for PeerInfoBehaviour {
    type ConnectionHandler = IntoConnectionHandlerSelect<
        <Ping as NetworkBehaviour>::ConnectionHandler,
        <Identify as NetworkBehaviour>::ConnectionHandler,
    >;
    type OutEvent = PeerInfoEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        IntoConnectionHandler::select(
            self.ping.new_handler(),
            self.identify.new_handler(),
        )
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        let mut list = self.ping.addresses_of_peer(peer_id);
        list.extend_from_slice(&self.identify.addresses_of_peer(peer_id));
        list
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                let ConnectionEstablished {
                    peer_id,
                    other_established,
                    ..
                } = connection_established;

                self.ping.on_swarm_event(FromSwarm::ConnectionEstablished(
                    connection_established,
                ));
                self.identify
                    .on_swarm_event(FromSwarm::ConnectionEstablished(
                        connection_established,
                    ));

                let addresses = self.addresses_of_peer(&peer_id);
                self.insert_peer_addresses(&peer_id, addresses);

                if other_established == 0 {
                    // this is the first connection to a given Peer
                    self.handle_initial_connection(peer_id);
                }
            }
            FromSwarm::ConnectionClosed(connection_closed) => {
                let ConnectionClosed {
                    remaining_established,
                    peer_id,
                    connection_id,
                    endpoint,
                    ..
                } = connection_closed;

                let (ping_handler, identity_handler) =
                    connection_closed.handler.into_inner();

                let ping_event = ConnectionClosed {
                    handler: ping_handler,
                    peer_id,
                    connection_id,
                    endpoint,
                    remaining_established,
                };
                self.ping
                    .on_swarm_event(FromSwarm::ConnectionClosed(ping_event));

                let identify_event = ConnectionClosed {
                    handler: identity_handler,
                    peer_id,
                    connection_id,
                    endpoint,
                    remaining_established,
                };

                self.identify
                    .on_swarm_event(FromSwarm::ConnectionClosed(identify_event));

                if remaining_established == 0 {
                    // this was the last connection to a given Peer
                    self.handle_last_connection(peer_id);
                }
            }
            FromSwarm::AddressChange(e) => {
                self.ping.on_swarm_event(FromSwarm::AddressChange(e));
                self.identify.on_swarm_event(FromSwarm::AddressChange(e));
            }
            FromSwarm::DialFailure(e) => {
                let (ping_handler, identity_handler) = e.handler.into_inner();
                let ping_event = DialFailure {
                    peer_id: e.peer_id,
                    handler: ping_handler,
                    error: e.error,
                };
                let identity_event = DialFailure {
                    peer_id: e.peer_id,
                    handler: identity_handler,
                    error: e.error,
                };
                self.ping.on_swarm_event(FromSwarm::DialFailure(ping_event));
                self.identify
                    .on_swarm_event(FromSwarm::DialFailure(identity_event));
            }
            FromSwarm::ListenFailure(e) => {
                let (ping_handler, identity_handler) = e.handler.into_inner();
                let ping_event = ListenFailure {
                    handler: ping_handler,
                    local_addr: e.local_addr,
                    send_back_addr: e.send_back_addr,
                };
                let identity_event = ListenFailure {
                    handler: identity_handler,
                    local_addr: e.local_addr,
                    send_back_addr: e.send_back_addr,
                };
                self.ping
                    .on_swarm_event(FromSwarm::ListenFailure(ping_event));
                self.identify
                    .on_swarm_event(FromSwarm::ListenFailure(identity_event));
            }
            FromSwarm::NewListener(e) => {
                self.ping.on_swarm_event(FromSwarm::NewListener(e));
                self.identify.on_swarm_event(FromSwarm::NewListener(e));
            }
            FromSwarm::ExpiredListenAddr(e) => {
                self.ping.on_swarm_event(FromSwarm::ExpiredListenAddr(e));
                self.identify
                    .on_swarm_event(FromSwarm::ExpiredListenAddr(e));
            }
            FromSwarm::ListenerError(e) => {
                self.ping.on_swarm_event(FromSwarm::ListenerError(e));
                self.identify.on_swarm_event(FromSwarm::ListenerError(e));
            }
            FromSwarm::ListenerClosed(e) => {
                self.ping.on_swarm_event(FromSwarm::ListenerClosed(e));
                self.identify.on_swarm_event(FromSwarm::ListenerClosed(e));
            }
            FromSwarm::NewExternalAddr(e) => {
                self.ping.on_swarm_event(FromSwarm::NewExternalAddr(e));
                self.identify.on_swarm_event(FromSwarm::NewExternalAddr(e));
            }
            FromSwarm::ExpiredExternalAddr(e) => {
                self.ping.on_swarm_event(FromSwarm::ExpiredExternalAddr(e));
                self.identify
                    .on_swarm_event(FromSwarm::ExpiredExternalAddr(e));
            }
            FromSwarm::NewListenAddr(e) => {
                self.ping.on_swarm_event(FromSwarm::NewListenAddr(e));
                self.identify.on_swarm_event(FromSwarm::NewListenAddr(e));
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        params: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event))
        }

        loop {
            match self.ping.poll(cx, params) {
                Poll::Pending => break,
                Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                    peer_id,
                    handler,
                    event,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                        peer_id,
                        handler,
                        event: EitherOutput::First(event),
                    })
                }
                Poll::Ready(NetworkBehaviourAction::ReportObservedAddr {
                    address,
                    score,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr {
                        address,
                        score,
                    })
                }
                Poll::Ready(NetworkBehaviourAction::CloseConnection {
                    peer_id,
                    connection,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::CloseConnection {
                        peer_id,
                        connection,
                    })
                }
                Poll::Ready(NetworkBehaviourAction::Dial { handler, opts }) => {
                    let handler = IntoConnectionHandler::select(
                        handler,
                        self.identify.new_handler(),
                    );

                    return Poll::Ready(NetworkBehaviourAction::Dial { handler, opts })
                }
                Poll::Ready(NetworkBehaviourAction::GenerateEvent(PingEvent {
                    peer,
                    result: Ok(PingSuccess::Ping { rtt }),
                })) => {
                    self.insert_latest_ping(&peer, rtt);
                    let event = PeerInfoEvent::PeerInfoUpdated { peer_id: peer };
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event))
                }
                _ => {}
            }
        }

        loop {
            match self.identify.poll(cx, params) {
                Poll::Pending => break,
                Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                    peer_id,
                    handler,
                    event,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                        peer_id,
                        handler,
                        event: EitherOutput::Second(event),
                    })
                }
                Poll::Ready(NetworkBehaviourAction::ReportObservedAddr {
                    address,
                    score,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr {
                        address,
                        score,
                    })
                }
                Poll::Ready(NetworkBehaviourAction::CloseConnection {
                    peer_id,
                    connection,
                }) => {
                    return Poll::Ready(NetworkBehaviourAction::CloseConnection {
                        peer_id,
                        connection,
                    })
                }
                Poll::Ready(NetworkBehaviourAction::Dial { handler, opts }) => {
                    let handler =
                        IntoConnectionHandler::select(self.ping.new_handler(), handler);
                    return Poll::Ready(NetworkBehaviourAction::Dial { handler, opts })
                }
                Poll::Ready(NetworkBehaviourAction::GenerateEvent(event)) => {
                    match event {
                        IdentifyEvent::Received {
                            peer_id,
                            info:
                                IdentifyInfo {
                                    protocol_version,
                                    agent_version,
                                    mut listen_addrs,
                                    ..
                                },
                        } => {
                            if listen_addrs.len() > MAX_IDENTIFY_ADDRESSES {
                                debug!(
                                    target: "fuel-libp2p",
                                    "Node {:?} has reported more than {} addresses; it is identified by {:?} and {:?}",
                                    peer_id, MAX_IDENTIFY_ADDRESSES, protocol_version, agent_version
                                );
                                listen_addrs.truncate(MAX_IDENTIFY_ADDRESSES);
                            }

                            self.insert_client_version(&peer_id, agent_version);
                            self.insert_peer_addresses(&peer_id, listen_addrs.clone());

                            let event = PeerInfoEvent::PeerIdentified {
                                peer_id,
                                addresses: listen_addrs,
                            };
                            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                                event,
                            ))
                        }
                        IdentifyEvent::Error { peer_id, error } => {
                            debug!(target: "fuel-libp2p", "Identification with peer {:?} failed => {}", peer_id, error)
                        }
                        _ => {}
                    }
                }
            }
        }

        Poll::Pending
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <<Self::ConnectionHandler as IntoConnectionHandler>::Handler as
            ConnectionHandler>::OutEvent,
    ) {
        match event {
            EitherOutput::First(ping_event) => {
                self.ping
                    .on_connection_handler_event(peer_id, connection_id, ping_event)
            }
            EitherOutput::Second(identify_event) => self
                .identify
                .on_connection_handler_event(peer_id, connection_id, identify_event),
        }
    }
}
