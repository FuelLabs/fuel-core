use crate::config::Config;
use libp2p::{
    core::{
        connection::ConnectionId,
        either::EitherOutput,
        ConnectedPoint,
        PublicKey,
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
    DialFailure,
    FromSwarm,
    ListenFailure,
};
use std::{
    collections::{
        HashMap,
        HashSet,
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
    PeerIdentified {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    PeerInfoUpdated {
        peer_id: PeerId,
    },
}

// Info about a single Peer that we're connected to
#[derive(Debug)]
pub struct PeerInfo {
    pub peer_addresses: HashSet<Multiaddr>,
    pub client_version: Option<String>,
    pub connected_point: ConnectedPoint,
    pub latest_ping: Option<Duration>,
}

impl PeerInfo {
    pub fn new(connected_point: ConnectedPoint) -> Self {
        Self {
            peer_addresses: HashSet::default(),
            client_version: None,
            connected_point,
            latest_ping: None,
        }
    }
}

// `Behaviour` that holds info about peers
pub struct PeerInfoBehaviour {
    ping: Ping,
    identify: Identify,
    peers: HashMap<PeerId, PeerInfo>,
}

impl PeerInfoBehaviour {
    pub fn new(local_public_key: PublicKey, config: &Config) -> Self {
        let identify = {
            let identify_config =
                IdentifyConfig::new("/fuel/1.0".to_string(), local_public_key);
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

        Self {
            ping,
            identify,
            peers: HashMap::default(),
        }
    }

    pub fn peers(&self) -> &HashMap<PeerId, PeerInfo> {
        &self.peers
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Insert peer addresses to a connected Node
    pub fn insert_peer_addresses(&mut self, peer_id: &PeerId, addresses: Vec<Multiaddr>) {
        match self.peers.get_mut(peer_id) {
            Some(peer_info) => {
                for address in addresses {
                    peer_info.peer_addresses.insert(address);
                }
            }
            _ => {
                debug!(target: "fuel-libp2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
            }
        }
    }

    /// Store newly connected peer
    fn insert_peer(&mut self, peer_id: &PeerId, connected_point: ConnectedPoint) {
        match self.peers.get_mut(peer_id) {
            Some(peer_info) => {
                peer_info.connected_point = connected_point;
            }
            _ => {
                self.peers.insert(*peer_id, PeerInfo::new(connected_point));
            }
        }
    }

    /// Insert client version to a connected Node
    fn insert_client_version(&mut self, peer_id: &PeerId, client_version: String) {
        match self.peers.get_mut(peer_id) {
            Some(peer_info) => {
                peer_info.client_version = Some(client_version);
            }
            _ => {
                debug!(target: "fuel-libp2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
            }
        }
    }

    /// Insert latest ping to a connected Node
    fn insert_latest_ping(&mut self, peer_id: &PeerId, duration: Duration) {
        if let Some(peer_info) = self.peers.get_mut(peer_id) {
            peer_info.latest_ping = Some(duration);
        } else {
            debug!(target: "fuel-libp2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
        }
    }
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

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<libp2p::Multiaddr> {
        let mut list = self.ping.addresses_of_peer(peer_id);
        list.extend_from_slice(&self.identify.addresses_of_peer(peer_id));
        list
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(e) => {
                self.ping
                    .on_swarm_event(FromSwarm::ConnectionEstablished(e));
                self.identify
                    .on_swarm_event(FromSwarm::ConnectionEstablished(e));
                self.insert_peer(&e.peer_id, e.endpoint.clone());

                let addresses = self.addresses_of_peer(&e.peer_id);
                self.insert_peer_addresses(&e.peer_id, addresses);
            }
            FromSwarm::ConnectionClosed(e) => {
                let (ping_handler, identity_handler) = e.handler.into_inner();
                let ping_event = ConnectionClosed {
                    handler: ping_handler,
                    peer_id: e.peer_id,
                    connection_id: e.connection_id,
                    endpoint: e.endpoint,
                    remaining_established: e.remaining_established,
                };
                self.ping
                    .on_swarm_event(FromSwarm::ConnectionClosed(ping_event));
                let identify_event = ConnectionClosed {
                    handler: identity_handler,
                    peer_id: e.peer_id,
                    connection_id: e.connection_id,
                    endpoint: e.endpoint,
                    remaining_established: e.remaining_established,
                };
                self.identify
                    .on_swarm_event(FromSwarm::ConnectionClosed(identify_event));
                self.peers.remove(&e.peer_id);
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
