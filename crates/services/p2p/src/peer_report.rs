use crate::{
    config::Config,
    heartbeat::{
        Heartbeat,
        HeartbeatEvent,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use libp2p::{
    self,
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
        PollParameters,
    },
    Multiaddr,
    PeerId,
};
use libp2p_core::Endpoint;
use libp2p_swarm::{
    derive_prelude::Either,
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
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

use tracing::debug;

/// Maximum amount of peer's addresses that we are ready to store per peer
const MAX_IDENTIFY_ADDRESSES: usize = 10;
const HEALTH_CHECK_INTERVAL_IN_SECONDS: u64 = 10;
const REPUTATION_DECAY_INTERVAL_IN_SECONDS: u64 = 1;

/// Events emitted by PeerReportBehavior
#[derive(Debug, Clone)]
pub enum PeerReportEvent {
    PeerConnected {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        initial_connection: bool,
    },
    PeerDisconnected {
        peer_id: PeerId,
    },
    PeerIdentified {
        peer_id: PeerId,
        agent_version: String,
        addresses: Vec<Multiaddr>,
    },
    PeerInfoUpdated {
        peer_id: PeerId,
        block_height: BlockHeight,
    },
    /// Informs p2p service / PeerManager to check health of reserved nodes' connections
    CheckReservedNodesHealth,
    /// Informs p2p service / PeerManager to perform reputation decay of connected nodes
    PerformDecay,
}

// `Behaviour` that reports events about peers
pub struct PeerReportBehaviour {
    heartbeat: Heartbeat,
    identify: Identify,
    pending_events: VecDeque<PeerReportEvent>,
    // regulary checks if reserved nodes are connected
    health_check: Interval,
    decay_interval: Interval,
}

impl PeerReportBehaviour {
    pub(crate) fn new(config: &Config) -> Self {
        let identify = {
            let identify_config =
                IdentifyConfig::new("/fuel/1.0".to_string(), config.keypair.public());
            if let Some(interval) = config.identify_interval {
                Identify::new(identify_config.with_interval(interval))
            } else {
                Identify::new(identify_config)
            }
        };

        let heartbeat =
            Heartbeat::new(config.heartbeat_config.clone(), BlockHeight::default());

        Self {
            heartbeat,
            identify,
            pending_events: VecDeque::default(),
            health_check: time::interval(Duration::from_secs(
                HEALTH_CHECK_INTERVAL_IN_SECONDS,
            )),
            decay_interval: time::interval(Duration::from_secs(
                REPUTATION_DECAY_INTERVAL_IN_SECONDS,
            )),
        }
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.heartbeat.update_block_height(block_height);
    }
}

impl NetworkBehaviour for PeerReportBehaviour {
    type ConnectionHandler = Either<
        <Heartbeat as NetworkBehaviour>::ConnectionHandler,
        <Identify as NetworkBehaviour>::ConnectionHandler,
    >;
    type ToSwarm = PeerReportEvent;

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

                let addresses = self.addresses_of_peer(&peer_id);
                self.pending_events
                    .push_back(PeerReportEvent::PeerConnected {
                        peer_id,
                        addresses,
                        initial_connection: other_established == 0,
                    });
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
                    self.pending_events
                        .push_back(PeerReportEvent::PeerDisconnected { peer_id })
                }
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
                    local_addr: e.local_addr,
                    send_back_addr: e.send_back_addr,
                    error: e.into(),
                    connection_id: e.connection_id,
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
            _ => {
                self.heartbeat.handle_swarm_event(&event);
                self.identify.handle_swarm_event(&event);
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        params: &mut impl PollParameters,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(event))
        }

        match self.heartbeat.poll(cx, params) {
            Poll::Pending => {}
            Poll::Ready(action) => {
                let action =
                    <PeerReportBehaviour as FromAction<Heartbeat>>::convert_action(
                        self, action,
                    );
                if let Some(action) = action {
                    return Poll::Ready(action)
                }
            }
        }

        loop {
            // poll until we've either exhausted the events or found one of interest
            match self.identify.poll(cx, params) {
                Poll::Pending => break,
                Poll::Ready(action) => {
                    if let Some(action) =
                        <PeerReportBehaviour as FromAction<Identify>>::convert_action(
                            self, action,
                        )
                    {
                        return Poll::Ready(action)
                    }
                }
            }
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

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(heartbeat_event) => self.heartbeat.on_connection_handler_event(
                peer_id,
                connection_id,
                heartbeat_event,
            ),
            Either::Right(identify_event) => self.identify.on_connection_handler_event(
                peer_id,
                connection_id,
                identify_event,
            ),
        }
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        todo!()
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        todo!()
    }
}

impl FromAction<Heartbeat> for PeerReportBehaviour {
    fn convert_action(
        &mut self,
        action: ToSwarm<
            <Heartbeat as NetworkBehaviour>::ToSwarm,
            <Heartbeat as NetworkBehaviour>::ConnectionHandler,
        >,
    ) -> Option<ToSwarm<Self::ToSwarm, Self::ConnectionHandler>> {
        match action {
            ToSwarm::GenerateEvent(HeartbeatEvent {
                peer_id,
                latest_block_height,
            }) => {
                let event = PeerReportEvent::PeerInfoUpdated {
                    peer_id,
                    block_height: latest_block_height,
                };
                Some(ToSwarm::GenerateEvent(event))
            }
            ToSwarm::Dial { opts } => Some(ToSwarm::Dial { opts }),
            ToSwarm::NotifyHandler {
                peer_id,
                handler,
                event,
            } => Some(ToSwarm::NotifyHandler {
                peer_id,
                handler,
                event: Either::Left(event),
            }),
            ToSwarm::CloseConnection {
                peer_id,
                connection,
            } => Some(ToSwarm::CloseConnection {
                peer_id,
                connection,
            }),
        }
    }
}

impl FromAction<Identify> for PeerReportBehaviour {
    fn convert_action(
        &mut self,
        action: ToSwarm<
            <Identify as NetworkBehaviour>::ToSwarm,
            <Identify as NetworkBehaviour>::ConnectionHandler,
        >,
    ) -> Option<ToSwarm<Self::ToSwarm, Self::ConnectionHandler>> {
        match action {
            ToSwarm::GenerateEvent(event) => match event {
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
                            target: "fuel-p2p",
                            "Node {:?} has reported more than {} addresses; it is identified by {:?} and {:?}",
                            peer_id, MAX_IDENTIFY_ADDRESSES, protocol_version, agent_version
                        );
                        listen_addrs.truncate(MAX_IDENTIFY_ADDRESSES);
                    }

                    let event = PeerReportEvent::PeerIdentified {
                        peer_id,
                        agent_version,
                        addresses: listen_addrs,
                    };

                    Some(ToSwarm::GenerateEvent(event))
                }
                IdentifyEvent::Error { peer_id, error } => {
                    debug!(target: "fuel-p2p", "Identification with peer {:?} failed => {}", peer_id, error);
                    None
                }
                _ => None,
            },
            ToSwarm::Dial { opts } => Some(ToSwarm::Dial { opts }),
            ToSwarm::NotifyHandler {
                peer_id,
                handler,
                event,
            } => Some(ToSwarm::NotifyHandler {
                peer_id,
                handler,
                event: Either::Right(event),
            }),
            ToSwarm::CloseConnection {
                peer_id,
                connection,
            } => Some(ToSwarm::CloseConnection {
                peer_id,
                connection,
            }),
        }
    }
}

trait FromAction<T: NetworkBehaviour>: NetworkBehaviour {
    fn convert_action(
        &mut self,
        action: ToSwarm<T::ToSwarm, T::ConnectionHandler>,
    ) -> Option<ToSwarm<Self::ToSwarm, Self::ConnectionHandler>>;
}

impl FromSwarmEvent for Heartbeat {}
impl FromSwarmEvent for Identify {}

trait FromSwarmEvent: NetworkBehaviour {
    fn handle_swarm_event(
        &mut self,
        event: &FromSwarm<<PeerReportBehaviour as NetworkBehaviour>::ConnectionHandler>,
    ) {
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
