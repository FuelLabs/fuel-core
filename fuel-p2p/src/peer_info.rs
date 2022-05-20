use crate::config::P2PConfig;
use libp2p::{
    core::{
        connection::{ConnectionId, ListenerId},
        either::EitherOutput,
        ConnectedPoint, PublicKey,
    },
    identify::{Identify, IdentifyConfig, IdentifyEvent, IdentifyInfo},
    ping::{Event as PingEvent, Ping, PingConfig, PingSuccess},
    swarm::{
        ConnectionHandler, IntoConnectionHandler, IntoConnectionHandlerSelect, NetworkBehaviour,
        NetworkBehaviourAction, PollParameters,
    },
    Multiaddr, PeerId,
};
use std::{
    collections::{HashMap, HashSet},
    task::{Context, Poll},
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
    pub fn new(local_public_key: PublicKey, config: &P2PConfig) -> Self {
        let identify = {
            let identify_config = IdentifyConfig::new("/fuel/1.0".to_string(), local_public_key);
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
        IntoConnectionHandler::select(self.ping.new_handler(), self.identify.new_handler())
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<libp2p::Multiaddr> {
        let mut list = self.ping.addresses_of_peer(peer_id);
        list.extend_from_slice(&self.identify.addresses_of_peer(peer_id));
        list
    }

    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        connection_id: &ConnectionId,
        connected_point: &ConnectedPoint,
        failed_addresses: Option<&Vec<Multiaddr>>,
        other_established: usize,
    ) {
        self.ping.inject_connection_established(
            peer_id,
            connection_id,
            connected_point,
            failed_addresses,
            other_established,
        );

        self.identify.inject_connection_established(
            peer_id,
            connection_id,
            connected_point,
            failed_addresses,
            other_established,
        );

        self.insert_peer(peer_id, connected_point.clone());

        let addresses = self.addresses_of_peer(peer_id);
        self.insert_peer_addresses(peer_id, addresses);
    }

    fn inject_connection_closed(
        &mut self,
        peer_id: &PeerId,
        conn: &ConnectionId,
        endpoint: &ConnectedPoint,
        handler: <Self::ConnectionHandler as IntoConnectionHandler>::Handler,
        remaining_established: usize,
    ) {
        let (ping_handler, identity_handler) = handler.into_inner();
        self.identify.inject_connection_closed(
            peer_id,
            conn,
            endpoint,
            identity_handler,
            remaining_established,
        );
        self.ping.inject_connection_closed(
            peer_id,
            conn,
            endpoint,
            ping_handler,
            remaining_established,
        );

        // todo: we could keep it in a cache for a while
        self.peers.remove(peer_id);
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
                Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address, score }) => {
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
                        IntoConnectionHandler::select(handler, self.identify.new_handler());

                    return Poll::Ready(NetworkBehaviourAction::Dial { handler, opts });
                }
                Poll::Ready(NetworkBehaviourAction::GenerateEvent(PingEvent {
                    peer,
                    result: Ok(PingSuccess::Ping { rtt }),
                })) => {
                    self.insert_latest_ping(&peer, rtt);
                    let event = PeerInfoEvent::PeerInfoUpdated { peer_id: peer };
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
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
                Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address, score }) => {
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
                    let handler = IntoConnectionHandler::select(self.ping.new_handler(), handler);
                    return Poll::Ready(NetworkBehaviourAction::Dial { handler, opts });
                }
                Poll::Ready(NetworkBehaviourAction::GenerateEvent(event)) => match event {
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
                        return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
                    }
                    IdentifyEvent::Error { peer_id, error } => {
                        debug!(target: "fuel-libp2p", "Identification with peer {:?} failed => {}", peer_id, error)
                    }
                    _ => {}
                },
            }
        }

        Poll::Pending
    }

    fn inject_event(
        &mut self,
        peer_id: PeerId,
        connection: ConnectionId,
        event: <<Self::ConnectionHandler as IntoConnectionHandler>::Handler as ConnectionHandler>::OutEvent,
    ) {
        match event {
            EitherOutput::First(ping_event) => {
                self.ping.inject_event(peer_id, connection, ping_event)
            }
            EitherOutput::Second(identify_event) => {
                self.identify
                    .inject_event(peer_id, connection, identify_event)
            }
        }
    }

    fn inject_address_change(
        &mut self,
        peer_id: &PeerId,
        conn: &ConnectionId,
        old: &ConnectedPoint,
        new: &ConnectedPoint,
    ) {
        self.ping.inject_address_change(peer_id, conn, old, new);
        self.identify.inject_address_change(peer_id, conn, old, new);
    }

    fn inject_dial_failure(
        &mut self,
        peer_id: Option<PeerId>,
        handler: Self::ConnectionHandler,
        error: &libp2p::swarm::DialError,
    ) {
        let (ping_handler, identity_handler) = handler.into_inner();
        self.identify
            .inject_dial_failure(peer_id, identity_handler, error);
        self.ping.inject_dial_failure(peer_id, ping_handler, error);
    }

    fn inject_new_listener(&mut self, id: ListenerId) {
        self.ping.inject_new_listener(id);
        self.identify.inject_new_listener(id);
    }

    fn inject_new_listen_addr(&mut self, id: ListenerId, addr: &Multiaddr) {
        self.ping.inject_new_listen_addr(id, addr);
        self.identify.inject_new_listen_addr(id, addr);
    }

    fn inject_expired_listen_addr(&mut self, id: ListenerId, addr: &Multiaddr) {
        self.ping.inject_expired_listen_addr(id, addr);
        self.identify.inject_expired_listen_addr(id, addr);
    }

    fn inject_new_external_addr(&mut self, addr: &Multiaddr) {
        self.ping.inject_new_external_addr(addr);
        self.identify.inject_new_external_addr(addr);
    }

    fn inject_expired_external_addr(&mut self, addr: &Multiaddr) {
        self.ping.inject_expired_external_addr(addr);
        self.identify.inject_expired_external_addr(addr);
    }

    fn inject_listen_failure(
        &mut self,
        local_addr: &Multiaddr,
        send_back_addr: &Multiaddr,
        handler: Self::ConnectionHandler,
    ) {
        let (ping_handler, identity_handler) = handler.into_inner();
        self.identify
            .inject_listen_failure(local_addr, send_back_addr, identity_handler);
        self.ping
            .inject_listen_failure(local_addr, send_back_addr, ping_handler);
    }

    fn inject_listener_error(&mut self, id: ListenerId, err: &(dyn std::error::Error + 'static)) {
        self.ping.inject_listener_error(id, err);
        self.identify.inject_listener_error(id, err);
    }

    fn inject_listener_closed(&mut self, id: ListenerId, reason: Result<(), &std::io::Error>) {
        self.ping.inject_listener_closed(id, reason);
        self.identify.inject_listener_closed(id, reason);
    }
}
