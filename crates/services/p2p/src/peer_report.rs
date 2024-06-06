use crate::{
    config::Config,
    TryPeerId,
};
use libp2p::{
    self,
    core::Endpoint,
    swarm::{
        derive_prelude::{
            ConnectionClosed,
            ConnectionEstablished,
            FromSwarm,
        },
        dial_opts::{
            DialOpts,
            PeerCondition,
        },
        dummy,
        ConnectionDenied,
        ConnectionId,
        NetworkBehaviour,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
    Multiaddr,
    PeerId,
};
use std::{
    collections::{
        BTreeMap,
        HashSet,
        VecDeque,
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
use tokio::time::{
    self,
    Interval,
};
use void::Void;

const HEALTH_CHECK_INTERVAL_IN_SECONDS: u64 = 10;
const REPUTATION_DECAY_INTERVAL_IN_SECONDS: u64 = 1;

/// Events emitted by PeerReportBehavior
#[derive(Debug, Clone)]
pub enum PeerReportEvent {
    PeerConnected {
        peer_id: PeerId,
    },
    PeerDisconnected {
        peer_id: PeerId,
    },
    /// Informs p2p service / PeerManager to perform reputation decay of connected nodes
    PerformDecay,
}

// `Behaviour` that reports events about peers
pub struct Behaviour {
    reserved_nodes_multiaddr: BTreeMap<PeerId, Vec<Multiaddr>>,
    reserved_nodes_to_connect: VecDeque<(Instant, PeerId)>,
    connected_reserved_nodes: HashSet<PeerId>,
    pending_connections: HashSet<ConnectionId>,
    pending_events: VecDeque<ToSwarm<PeerReportEvent, Void>>,
    decay_interval: Interval,
}

impl Behaviour {
    pub(crate) fn new(_config: &Config) -> Self {
        let mut reserved_nodes_to_connect = VecDeque::new();
        let mut reserved_nodes_multiaddr = BTreeMap::<PeerId, Vec<Multiaddr>>::new();

        for multiaddr in &_config.reserved_nodes {
            let peer_id = multiaddr.try_to_peer_id().unwrap();
            reserved_nodes_to_connect.push_back((Instant::now(), peer_id));
            reserved_nodes_multiaddr
                .entry(peer_id)
                .or_default()
                .push(multiaddr.clone());
        }

        Self {
            reserved_nodes_to_connect,
            reserved_nodes_multiaddr,
            connected_reserved_nodes: Default::default(),
            pending_connections: Default::default(),
            pending_events: VecDeque::default(),
            decay_interval: time::interval(Duration::from_secs(
                REPUTATION_DECAY_INTERVAL_IN_SECONDS,
            )),
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = PeerReportEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                let ConnectionEstablished {
                    peer_id,
                    connection_id,
                    ..
                } = connection_established;
                self.pending_events.push_back(ToSwarm::GenerateEvent(
                    PeerReportEvent::PeerConnected { peer_id },
                ));
                if self.reserved_nodes_multiaddr.contains_key(&peer_id) {
                    self.connected_reserved_nodes.insert(peer_id);
                    self.pending_connections.remove(&connection_id);
                }
            }
            FromSwarm::ConnectionClosed(connection_closed) => {
                let ConnectionClosed {
                    remaining_established,
                    peer_id,
                    ..
                } = connection_closed;

                if remaining_established == 0 {
                    // this was the last connection to a given Peer
                    self.pending_events.push_back(ToSwarm::GenerateEvent(
                        PeerReportEvent::PeerDisconnected { peer_id },
                    ));

                    if self.reserved_nodes_multiaddr.contains_key(&peer_id) {
                        self.connected_reserved_nodes.remove(&peer_id);
                        self.reserved_nodes_to_connect
                            .push_back((Instant::now(), peer_id));
                    }
                }
            }
            FromSwarm::DialFailure(dial) => {
                tracing::error!(
                    "Dial failure: peer id `{:?}` with error `{}`",
                    dial.peer_id,
                    dial.error
                );
                if let Some(peer_id) = dial.peer_id {
                    if self.pending_connections.remove(&dial.connection_id)
                        && !self.connected_reserved_nodes.contains(&peer_id)
                    {
                        self.reserved_nodes_to_connect
                            .push_back((Instant::now(), peer_id));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event)
        }

        if let Some((instant, peer_id)) = self.reserved_nodes_to_connect.front() {
            if instant.elapsed() > Duration::from_secs(HEALTH_CHECK_INTERVAL_IN_SECONDS) {
                let peer_id = *peer_id;
                self.reserved_nodes_to_connect.pop_front();
                // The initial DNS address can be replaced with a real IP, but when
                // the node disconnects, the IP may change. Using initial multiaddrs
                // here allows you to reconnect and get a new IP again.
                let multiaddrs = self
                    .reserved_nodes_multiaddr
                    .get(&peer_id)
                    .expect("Multiaddr is always available")
                    .clone();
                let opts = DialOpts::peer_id(peer_id)
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .addresses(multiaddrs)
                    .build();
                self.pending_connections.insert(opts.connection_id());

                return Poll::Ready(ToSwarm::Dial { opts })
            }
        }

        if self.decay_interval.poll_tick(cx).is_ready() {
            return Poll::Ready(ToSwarm::GenerateEvent(PeerReportEvent::PerformDecay))
        }

        Poll::Pending
    }
}
