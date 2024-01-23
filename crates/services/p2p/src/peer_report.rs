use crate::config::Config;
use libp2p::{
    self,
    core::Endpoint,
    identify,
    swarm::{
        derive_prelude::{
            ConnectionClosed,
            ConnectionEstablished,
            FromSwarm,
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
    collections::VecDeque,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};
use tokio::time::{
    self,
    Interval,
};

const HEALTH_CHECK_INTERVAL_IN_SECONDS: u64 = 10;
const REPUTATION_DECAY_INTERVAL_IN_SECONDS: u64 = 1;

/// Events emitted by PeerReportBehavior
#[derive(Debug, Clone)]
pub enum PeerReportEvent {
    PeerConnected {
        peer_id: PeerId,
        initial_connection: bool,
    },
    PeerDisconnected {
        peer_id: PeerId,
    },
    /// Informs p2p service / PeerManager to check health of reserved nodes' connections
    CheckReservedNodesHealth,
    /// Informs p2p service / PeerManager to perform reputation decay of connected nodes
    PerformDecay,
}

// `Behaviour` that reports events about peers
pub struct Behaviour {
    pending_events: VecDeque<PeerReportEvent>,
    // regulary checks if reserved nodes are connected
    health_check: Interval,
    decay_interval: Interval,
}

impl Behaviour {
    pub(crate) fn new(_config: &Config) -> Self {
        Self {
            pending_events: VecDeque::default(),
            health_check: time::interval(Duration::from_secs(
                HEALTH_CHECK_INTERVAL_IN_SECONDS,
            )),
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
                    other_established,
                    ..
                } = connection_established;
                self.pending_events
                    .push_back(PeerReportEvent::PeerConnected {
                        peer_id,
                        initial_connection: other_established == 0,
                    });
            }
            FromSwarm::ConnectionClosed(connection_closed) => {
                let ConnectionClosed {
                    remaining_established,
                    peer_id,
                    ..
                } = connection_closed;

                if remaining_established == 0 {
                    // this was the last connection to a given Peer
                    self.pending_events
                        .push_back(PeerReportEvent::PeerDisconnected { peer_id })
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
            return Poll::Ready(ToSwarm::GenerateEvent(event))
        }

        if self.decay_interval.poll_tick(cx).is_ready() {
            return Poll::Ready(ToSwarm::GenerateEvent(PeerReportEvent::PerformDecay))
        }

        if self.health_check.poll_tick(cx).is_ready() {
            return Poll::Ready(ToSwarm::GenerateEvent(
                PeerReportEvent::CheckReservedNodesHealth,
            ))
        }

        Poll::Pending
    }
}

trait FromAction<T: NetworkBehaviour>: NetworkBehaviour {
    fn convert_action(
        &mut self,
        action: ToSwarm<T::ToSwarm, THandlerInEvent<T>>,
    ) -> Option<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>>;
}

impl FromSwarmEvent for Behaviour {}
impl FromSwarmEvent for identify::Behaviour {}

trait FromSwarmEvent: NetworkBehaviour {
    fn handle_swarm_event(&mut self, event: &FromSwarm) {
        match event {
            FromSwarm::NewListener(e) => {
                self.on_swarm_event(FromSwarm::NewListener(*e));
            }
            FromSwarm::ExpiredListenAddr(e) => {
                self.on_swarm_event(FromSwarm::ExpiredListenAddr(*e));
            }
            FromSwarm::ListenerError(e) => {
                self.on_swarm_event(FromSwarm::ListenerError(*e));
            }
            FromSwarm::ListenerClosed(e) => {
                self.on_swarm_event(FromSwarm::ListenerClosed(*e));
            }
            FromSwarm::NewExternalAddrCandidate(e) => {
                self.on_swarm_event(FromSwarm::NewExternalAddrCandidate(*e));
            }
            FromSwarm::ExternalAddrExpired(e) => {
                self.on_swarm_event(FromSwarm::ExternalAddrExpired(*e));
            }
            FromSwarm::NewListenAddr(e) => {
                self.on_swarm_event(FromSwarm::NewListenAddr(*e));
            }
            FromSwarm::AddressChange(e) => {
                self.on_swarm_event(FromSwarm::AddressChange(*e));
            }
            _ => {}
        }
    }
}
