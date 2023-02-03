use crate::{
    config::Config,
    heartbeat::{
        Heartbeat,
        HeartbeatConfig,
        HeartbeatEvent,
    },
};
use fuel_core_types::blockchain::primitives::BlockHeight;
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
    swarm::{
        derive_prelude::{
            ConnectionClosed,
            ConnectionEstablished,
            DialFailure,
            FromSwarm,
            ListenFailure,
        },
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
use rand::seq::IteratorRandom;

use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    sync::{
        Arc,
        RwLock,
    },
    task::{
        Context,
        Poll,
    },
    time::{
        Duration,
        Instant,
    },
};
use tokio::time::Interval;
use tracing::debug;

/// Maximum amount of peer's addresses that we are ready to store per peer
const MAX_IDENTIFY_ADDRESSES: usize = 10;
const HEALTH_CHECK_INTERVAL_IN_SECONDS: u64 = 10;

/// Events emitted by PeerInfoBehaviour
#[derive(Debug, Clone)]
pub enum PeerInfoEvent {
    PeerConnected(PeerId),
    PeerDisconnected {
        peer_id: PeerId,
        should_reconnect: bool,
    },
    TooManyPeers {
        peer_to_disconnect: PeerId,
    },
    ReconnectToPeer(PeerId),
    PeerIdentified {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    PeerInfoUpdated {
        peer_id: PeerId,
        block_height: BlockHeight,
    },
}

// `Behaviour` that holds info about peers
pub struct PeerManagerBehaviour {
    heartbeat: Heartbeat,
    identify: Identify,
    peer_manager: PeerManager,
    // regulary checks if reserved nodes are connected
    health_check: Interval,
}

impl PeerManagerBehaviour {
    pub(crate) fn new(
        config: &Config,
        connection_state: Arc<RwLock<ConnectionState>>,
    ) -> Self {
        let identify = {
            let identify_config =
                IdentifyConfig::new("/fuel/1.0".to_string(), config.keypair.public());
            if let Some(interval) = config.identify_interval {
                Identify::new(identify_config.with_interval(interval))
            } else {
                Identify::new(identify_config)
            }
        };

        let heartbeat = Heartbeat::new(HeartbeatConfig::new(), BlockHeight::default());

        let reserved_peers: HashSet<PeerId> = config
            .reserved_nodes
            .iter()
            .filter_map(PeerId::try_from_multiaddr)
            .collect();

        let peer_manager = PeerManager::new(
            reserved_peers,
            connection_state,
            config.max_peers_connected as usize,
        );

        Self {
            heartbeat,
            identify,
            peer_manager,
            health_check: tokio::time::interval(Duration::from_secs(
                HEALTH_CHECK_INTERVAL_IN_SECONDS,
            )),
        }
    }

    pub fn total_peers_connected(&self) -> usize {
        self.peer_manager.total_peers_connected()
    }

    /// returns an iterator over the connected peers
    pub fn get_peers_ids(&self) -> impl Iterator<Item = &PeerId> {
        self.peer_manager.get_peers_ids()
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peer_manager.get_peer_info(peer_id)
    }

    pub fn insert_peer_addresses(&mut self, peer_id: &PeerId, addresses: Vec<Multiaddr>) {
        self.peer_manager
            .insert_peer_info(peer_id, PeerInfoInsert::Addresses(addresses));
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.heartbeat.update_block_height(block_height);
    }

    /// Find a peer that is holding the given block height.
    pub fn get_peer_id_with_height(&self, height: &BlockHeight) -> Option<PeerId> {
        self.peer_manager.get_peer_id_with_height(height)
    }
}

impl NetworkBehaviour for PeerManagerBehaviour {
    type ConnectionHandler = IntoConnectionHandlerSelect<
        <Heartbeat as NetworkBehaviour>::ConnectionHandler,
        <Identify as NetworkBehaviour>::ConnectionHandler,
    >;
    type OutEvent = PeerInfoEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        IntoConnectionHandler::select(
            self.heartbeat.new_handler(),
            self.identify.new_handler(),
        )
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        self.identify.addresses_of_peer(peer_id)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                let ConnectionEstablished {
                    peer_id,
                    other_established,
                    ..
                } = connection_established;

                self.heartbeat
                    .on_swarm_event(FromSwarm::ConnectionEstablished(
                        connection_established,
                    ));
                self.identify
                    .on_swarm_event(FromSwarm::ConnectionEstablished(
                        connection_established,
                    ));

                if other_established == 0 {
                    // this is the first connection to a given Peer
                    self.peer_manager.handle_initial_connection(peer_id);
                }

                let addresses = self.addresses_of_peer(&peer_id);
                self.insert_peer_addresses(&peer_id, addresses);
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
                self.heartbeat
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
                    self.peer_manager.handle_peer_disconnect(peer_id);
                }
            }
            FromSwarm::AddressChange(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::AddressChange(e));
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
                self.heartbeat
                    .on_swarm_event(FromSwarm::DialFailure(ping_event));
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
                self.heartbeat
                    .on_swarm_event(FromSwarm::ListenFailure(ping_event));
                self.identify
                    .on_swarm_event(FromSwarm::ListenFailure(identity_event));
            }
            FromSwarm::NewListener(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::NewListener(e));
                self.identify.on_swarm_event(FromSwarm::NewListener(e));
            }
            FromSwarm::ExpiredListenAddr(e) => {
                self.heartbeat
                    .on_swarm_event(FromSwarm::ExpiredListenAddr(e));
                self.identify
                    .on_swarm_event(FromSwarm::ExpiredListenAddr(e));
            }
            FromSwarm::ListenerError(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::ListenerError(e));
                self.identify.on_swarm_event(FromSwarm::ListenerError(e));
            }
            FromSwarm::ListenerClosed(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::ListenerClosed(e));
                self.identify.on_swarm_event(FromSwarm::ListenerClosed(e));
            }
            FromSwarm::NewExternalAddr(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::NewExternalAddr(e));
                self.identify.on_swarm_event(FromSwarm::NewExternalAddr(e));
            }
            FromSwarm::ExpiredExternalAddr(e) => {
                self.heartbeat
                    .on_swarm_event(FromSwarm::ExpiredExternalAddr(e));
                self.identify
                    .on_swarm_event(FromSwarm::ExpiredExternalAddr(e));
            }
            FromSwarm::NewListenAddr(e) => {
                self.heartbeat.on_swarm_event(FromSwarm::NewListenAddr(e));
                self.identify.on_swarm_event(FromSwarm::NewListenAddr(e));
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        params: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if self.health_check.poll_tick(cx).is_ready() {
            let disconnected_peers: Vec<_> = self
                .peer_manager
                .get_disconnected_reserved_peers()
                .copied()
                .collect();

            for peer_id in disconnected_peers {
                debug!(target: "fuel-libp2p", "Trying to reconnect to reserved peer {:?}", peer_id);

                self.peer_manager
                    .pending_events
                    .push_back(PeerInfoEvent::ReconnectToPeer(peer_id));
            }
        }

        if let Some(event) = self.peer_manager.pending_events.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event))
        }

        match self.heartbeat.poll(cx, params) {
            Poll::Pending => {}
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
                let handler =
                    IntoConnectionHandler::select(handler, self.identify.new_handler());

                return Poll::Ready(NetworkBehaviourAction::Dial { handler, opts })
            }
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(HeartbeatEvent {
                peer_id,
                latest_block_height,
            })) => {
                let heartbeat_data = HeartbeatData::new(latest_block_height);

                if let Some(previous_heartbeat) = self
                    .get_peer_info(&peer_id)
                    .and_then(|info| info.heartbeat_data.seconds_since_last_heartbeat())
                {
                    debug!(target: "fuel-libp2p", "Previous hearbeat happened {:?} seconds ago", previous_heartbeat);
                }

                self.peer_manager.insert_peer_info(
                    &peer_id,
                    PeerInfoInsert::HeartbeatData(heartbeat_data),
                );

                let event = PeerInfoEvent::PeerInfoUpdated {
                    peer_id,
                    block_height: latest_block_height,
                };
                return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event))
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
                    let handler = IntoConnectionHandler::select(
                        self.heartbeat.new_handler(),
                        handler,
                    );
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

                            self.peer_manager.insert_peer_info(
                                &peer_id,
                                PeerInfoInsert::ClientVersion(agent_version),
                            );
                            self.peer_manager.insert_peer_info(
                                &peer_id,
                                PeerInfoInsert::Addresses(listen_addrs.clone()),
                            );

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
            EitherOutput::First(heartbeat_event) => self
                .heartbeat
                .on_connection_handler_event(peer_id, connection_id, heartbeat_event),
            EitherOutput::Second(identify_event) => self
                .identify
                .on_connection_handler_event(peer_id, connection_id, identify_event),
        }
    }
}

// Info about a single Peer that we're connected to
#[derive(Debug, Default, Clone)]
pub struct PeerInfo {
    pub peer_addresses: HashSet<Multiaddr>,
    pub client_version: Option<String>,
    pub heartbeat_data: HeartbeatData,
}

enum PeerInfoInsert {
    Addresses(Vec<Multiaddr>),
    ClientVersion(String),
    HeartbeatData(HeartbeatData),
}

/// Manages Peers and their events
#[derive(Debug, Default, Clone)]
struct PeerManager {
    pending_events: VecDeque<PeerInfoEvent>,
    non_reserved_connected_peers: HashMap<PeerId, PeerInfo>,
    reserved_connected_peers: HashMap<PeerId, PeerInfo>,
    reserved_peers: HashSet<PeerId>,
    connection_state: Arc<RwLock<ConnectionState>>,
    max_non_reserved_peers: usize,
}

impl PeerManager {
    fn new(
        reserved_peers: HashSet<PeerId>,
        connection_state: Arc<RwLock<ConnectionState>>,
        max_non_reserved_peers: usize,
    ) -> Self {
        Self {
            pending_events: VecDeque::default(),
            non_reserved_connected_peers: HashMap::with_capacity(max_non_reserved_peers),
            reserved_connected_peers: HashMap::with_capacity(reserved_peers.len()),
            reserved_peers,
            connection_state,
            max_non_reserved_peers,
        }
    }

    fn total_peers_connected(&self) -> usize {
        self.reserved_connected_peers.len() + self.non_reserved_connected_peers.len()
    }

    fn get_peers_ids(&self) -> impl Iterator<Item = &PeerId> {
        self.non_reserved_connected_peers
            .keys()
            .chain(self.reserved_connected_peers.keys())
    }

    fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        if self.reserved_peers.contains(peer_id) {
            return self.reserved_connected_peers.get(peer_id)
        }
        self.non_reserved_connected_peers.get(peer_id)
    }

    fn insert_peer_info(&mut self, peer_id: &PeerId, data: PeerInfoInsert) {
        let peers = if self.reserved_peers.contains(peer_id) {
            &mut self.reserved_connected_peers
        } else {
            &mut self.non_reserved_connected_peers
        };
        match data {
            PeerInfoInsert::Addresses(addresses) => {
                insert_peer_addresses(peers, peer_id, addresses)
            }
            PeerInfoInsert::ClientVersion(client_version) => {
                insert_client_version(peers, peer_id, client_version)
            }
            PeerInfoInsert::HeartbeatData(block_height) => {
                insert_heartbeat_data(peers, peer_id, block_height)
            }
        }
    }

    fn get_disconnected_reserved_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.reserved_peers
            .iter()
            .filter(|peer_id| !self.reserved_connected_peers.contains_key(peer_id))
    }

    /// Handles the first connnection established with a Peer
    fn handle_initial_connection(&mut self, peer_id: PeerId) {
        let non_reserved_peers_connected = self.non_reserved_connected_peers.len();

        // if the connected Peer is not from the reserved peers
        if !self.reserved_peers.contains(&peer_id) {
            // check if all the slots are already taken
            if non_reserved_peers_connected >= self.max_non_reserved_peers {
                // Too many peers already connected, disconnect the Peer with the first priority.
                self.pending_events.push_front(PeerInfoEvent::TooManyPeers {
                    peer_to_disconnect: peer_id,
                });

                // early exit, we don't want to report new peer connection
                // nor insert it in `connected_peers`
                // since we're going to disconnect it anyways
                return
            }

            if non_reserved_peers_connected + 1 == self.max_non_reserved_peers {
                // this is the last non-reserved peer allowed
                if let Ok(mut connection_state) = self.connection_state.write() {
                    connection_state.deny_new_peers();
                }
            }

            self.non_reserved_connected_peers
                .insert(peer_id, PeerInfo::default());
        } else {
            self.reserved_connected_peers
                .insert(peer_id, PeerInfo::default());
        }

        self.pending_events
            .push_back(PeerInfoEvent::PeerConnected(peer_id));
    }

    /// Handles on peer's last connection getting disconnected
    fn handle_peer_disconnect(&mut self, peer_id: PeerId) {
        // try immediate reconnect if it's a reserved peer
        let is_reserved = self.reserved_peers.contains(&peer_id);
        let is_removed;

        if !is_reserved {
            is_removed = self.non_reserved_connected_peers.remove(&peer_id).is_some();

            // check were all the slots full prior to this disconnect
            if is_removed
                && self.max_non_reserved_peers
                    == self.non_reserved_connected_peers.len() + 1
            {
                // since all the slots were full prior to this disconnect
                // let's allow new peer non-reserved peers connections
                if let Ok(mut connection_state) = self.connection_state.write() {
                    connection_state.allow_new_peers();
                }
            }
        } else {
            is_removed = self.reserved_connected_peers.remove(&peer_id).is_some();
        }

        if is_removed {
            self.pending_events
                .push_back(PeerInfoEvent::PeerDisconnected {
                    peer_id,
                    should_reconnect: is_reserved,
                })
        }
    }

    /// Find a peer that is holding the given block height.
    pub fn get_peer_id_with_height(&self, height: &BlockHeight) -> Option<PeerId> {
        let mut range = rand::thread_rng();
        // TODO: Optimize the selection of the peer.
        //  We can store pair `(peer id, height)` for all nodes(reserved and not) in the
        //  https://docs.rs/sorted-vec/latest/sorted_vec/struct.SortedVec.html
        self.non_reserved_connected_peers
            .iter()
            .chain(self.reserved_connected_peers.iter())
            .filter(|(_, peer_info)| {
                peer_info.heartbeat_data.block_height >= Some(*height)
            })
            .map(|(peer_id, _)| *peer_id)
            .choose(&mut range)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ConnectionState {
    peers_allowed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HeartbeatData {
    pub block_height: Option<BlockHeight>,
    pub last_heartbeat: Option<Instant>,
}

impl HeartbeatData {
    fn new(block_height: BlockHeight) -> Self {
        Self {
            block_height: Some(block_height),
            last_heartbeat: Some(Instant::now()),
        }
    }

    pub fn seconds_since_last_heartbeat(&self) -> Option<Duration> {
        self.last_heartbeat.map(|time| time.elapsed())
    }
}

impl ConnectionState {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            peers_allowed: true,
        }))
    }

    pub fn available_slot(&self) -> bool {
        self.peers_allowed
    }

    fn allow_new_peers(&mut self) {
        self.peers_allowed = true;
    }

    fn deny_new_peers(&mut self) {
        self.peers_allowed = false;
    }
}

fn insert_peer_addresses(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    addresses: Vec<Multiaddr>,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        for address in addresses {
            peer.peer_addresses.insert(address);
        }
    } else {
        log_missing_peer(peer_id);
    }
}

fn insert_heartbeat_data(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    heartbeat_data: HeartbeatData,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        peer.heartbeat_data = heartbeat_data;
    } else {
        log_missing_peer(peer_id);
    }
}

fn insert_client_version(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    client_version: String,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        peer.client_version = Some(client_version);
    } else {
        log_missing_peer(peer_id);
    }
}

fn log_missing_peer(peer_id: &PeerId) {
    debug!(target: "fuel-libp2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_random_peers(size: usize) -> Vec<PeerId> {
        (0..size).map(|_| PeerId::random()).collect()
    }

    fn initialize_peer_manager(
        reserved_peers: Vec<PeerId>,
        max_non_reserved_peers: usize,
    ) -> PeerManager {
        let connection_state = ConnectionState::new();

        PeerManager::new(
            reserved_peers.into_iter().collect(),
            connection_state,
            max_non_reserved_peers,
        )
    }

    #[test]
    fn only_allowed_number_of_non_reserved_peers_is_connected() {
        let max_non_reserved_peers = 5;
        let mut peer_manager = initialize_peer_manager(vec![], max_non_reserved_peers);

        let random_peers = get_random_peers(max_non_reserved_peers * 2);

        // try connecting all the random peers
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(*peer_id);
        }

        assert_eq!(peer_manager.total_peers_connected(), max_non_reserved_peers);
    }

    #[test]
    fn only_reserved_peers_are_connected() {
        let max_non_reserved_peers = 0;
        let reserved_peers = get_random_peers(5);
        let mut peer_manager =
            initialize_peer_manager(reserved_peers.clone(), max_non_reserved_peers);

        // try connecting all the reserved peers
        for peer_id in &reserved_peers {
            peer_manager.handle_initial_connection(*peer_id);
        }

        assert_eq!(peer_manager.total_peers_connected(), reserved_peers.len());

        // try connecting random peers
        let random_peers = get_random_peers(10);
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(*peer_id);
        }

        // the number should stay the same
        assert_eq!(peer_manager.total_peers_connected(), reserved_peers.len());
    }

    #[test]
    fn non_reserved_peer_does_not_take_reserved_slot() {
        let max_non_reserved_peers = 5;
        let reserved_peers = get_random_peers(5);
        let mut peer_manager =
            initialize_peer_manager(reserved_peers.clone(), max_non_reserved_peers);

        // try connecting all the reserved peers
        for peer_id in &reserved_peers {
            peer_manager.handle_initial_connection(*peer_id);
        }

        // disconnect a single reserved peer
        peer_manager.handle_peer_disconnect(*reserved_peers.first().unwrap());

        // try connecting random peers
        let random_peers = get_random_peers(max_non_reserved_peers * 2);
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(*peer_id);
        }

        // there should be an available slot for a reserved peer
        assert_eq!(
            peer_manager.total_peers_connected(),
            reserved_peers.len() - 1 + max_non_reserved_peers
        );

        // reconnect the disconnected reserved peer
        peer_manager.handle_initial_connection(*reserved_peers.first().unwrap());

        // all the slots should be taken now
        assert_eq!(
            peer_manager.total_peers_connected(),
            reserved_peers.len() + max_non_reserved_peers
        );
    }
}
