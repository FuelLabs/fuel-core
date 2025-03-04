//! Copy of the original `connection_limits::Behaviour` with additional logic around
//! reserved nodes and [`LimitedBehaviour`].

use libp2p::{
    core::{
        transport::PortUse,
        ConnectedPoint,
        Endpoint,
        Multiaddr,
    },
    identity::PeerId,
    swarm::{
        behaviour::{
            ConnectionEstablished,
            DialFailure,
            ListenFailure,
        },
        dummy,
        ConnectionClosed,
        ConnectionDenied,
        ConnectionId,
        FromSwarm,
        NetworkBehaviour,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
};
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
        HashSet,
    },
    fmt,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};
use void::Void;

#[derive(Default)]
pub struct ConnectionsStatistic {
    pub pending_inbound_connections: HashSet<ConnectionId>,
    pub pending_outbound_connections: HashSet<ConnectionId>,
    pub established_inbound_connections: HashSet<ConnectionId>,
    pub established_outbound_connections: HashSet<ConnectionId>,
    pub established_per_peer: HashMap<PeerId, HashSet<ConnectionId>>,
    pub reserved_peers: HashSet<PeerId>,
}

pub type Connections = Arc<parking_lot::Mutex<ConnectionsStatistic>>;

pub struct Behaviour {
    limits: ConnectionLimits,
    connections: Connections,
}

impl Behaviour {
    pub fn new(limits: ConnectionLimits, reserved_peers: HashSet<PeerId>) -> Self {
        let connections = ConnectionsStatistic {
            reserved_peers,
            ..Default::default()
        };
        Self {
            limits,
            connections: Arc::new(parking_lot::Mutex::new(connections)),
        }
    }

    pub fn connections(&self) -> Connections {
        self.connections.clone()
    }

    /// Returns a mutable reference to [`ConnectionLimits`].
    /// > **Note**: A new limit will not be enforced against existing connections.
    pub fn limits_mut(&mut self) -> &mut ConnectionLimits {
        &mut self.limits
    }
}

fn check_limit(
    limit: Option<u32>,
    current: usize,
    kind: Kind,
) -> Result<(), ConnectionDenied> {
    let limit = limit.unwrap_or(u32::MAX);
    let current = u32::try_from(current).unwrap_or(u32::MAX);

    if current >= limit {
        return Err(ConnectionDenied::new(Exceeded { limit, kind }));
    }

    Ok(())
}

/// A connection limit has been exceeded.
#[derive(Debug, Clone, Copy)]
pub struct Exceeded {
    limit: u32,
    kind: Kind,
}

impl fmt::Display for Exceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "connection limit exceeded: at most {} {} are allowed",
            self.limit, self.kind
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    PendingIncoming,
    PendingOutgoing,
    EstablishedIncoming,
    EstablishedOutgoing,
    EstablishedPerPeer,
    EstablishedTotal,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::PendingIncoming => write!(f, "pending incoming connections"),
            Kind::PendingOutgoing => write!(f, "pending outgoing connections"),
            Kind::EstablishedIncoming => write!(f, "established incoming connections"),
            Kind::EstablishedOutgoing => write!(f, "established outgoing connections"),
            Kind::EstablishedPerPeer => write!(f, "established connections per peer"),
            Kind::EstablishedTotal => write!(f, "established connections"),
        }
    }
}

impl std::error::Error for Exceeded {}

/// The configurable connection limits.
#[derive(Debug, Clone, Default)]
pub struct ConnectionLimits {
    max_pending_incoming: Option<u32>,
    max_pending_outgoing: Option<u32>,
    max_established_incoming: Option<u32>,
    max_established_outgoing: Option<u32>,
    max_established_per_peer: Option<u32>,
    max_established_total: Option<u32>,
}

impl ConnectionLimits {
    /// Configures the maximum number of concurrently incoming connections being established.
    pub fn with_max_pending_incoming(mut self, limit: Option<u32>) -> Self {
        self.max_pending_incoming = limit;
        self
    }

    /// Configures the maximum number of concurrently outgoing connections being established.
    pub fn with_max_pending_outgoing(mut self, limit: Option<u32>) -> Self {
        self.max_pending_outgoing = limit;
        self
    }

    /// Configures the maximum number of concurrent established inbound connections.
    pub fn with_max_established_incoming(mut self, limit: Option<u32>) -> Self {
        self.max_established_incoming = limit;
        self
    }

    /// Configures the maximum number of concurrent established outbound connections.
    pub fn with_max_established_outgoing(mut self, limit: Option<u32>) -> Self {
        self.max_established_outgoing = limit;
        self
    }

    /// Configures the maximum number of concurrent established connections (both
    /// inbound and outbound).
    ///
    /// Note: This should be used in conjunction with
    /// [`ConnectionLimits::with_max_established_incoming`] to prevent possible
    /// eclipse attacks (all connections being inbound).
    pub fn with_max_established(mut self, limit: Option<u32>) -> Self {
        self.max_established_total = limit;
        self
    }

    /// Configures the maximum number of concurrent established connections per peer,
    /// regardless of direction (incoming or outgoing).
    pub fn with_max_established_per_peer(mut self, limit: Option<u32>) -> Self {
        self.max_established_per_peer = limit;
        self
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Void;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        let mut connections = self.connections.lock();

        check_limit(
            self.limits.max_pending_incoming,
            connections.pending_inbound_connections.len(),
            Kind::PendingIncoming,
        )?;

        connections
            .pending_inbound_connections
            .insert(connection_id);

        Ok(())
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mut connections = self.connections.lock();

        connections
            .pending_inbound_connections
            .remove(&connection_id);

        if connections.reserved_peers.contains(&peer) {
            return Ok(dummy::ConnectionHandler)
        }

        check_limit(
            self.limits.max_established_incoming,
            connections.established_inbound_connections.len(),
            Kind::EstablishedIncoming,
        )?;
        check_limit(
            self.limits.max_established_per_peer,
            connections
                .established_per_peer
                .get(&peer)
                .map(|connections| connections.len())
                .unwrap_or(0),
            Kind::EstablishedPerPeer,
        )?;
        let total = connections
            .established_inbound_connections
            .len()
            .saturating_add(connections.established_outbound_connections.len());
        check_limit(
            self.limits.max_established_total,
            total,
            Kind::EstablishedTotal,
        )?;

        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        _: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut connections = self.connections.lock();
        check_limit(
            self.limits.max_pending_outgoing,
            connections.pending_outbound_connections.len(),
            Kind::PendingOutgoing,
        )?;

        connections
            .pending_outbound_connections
            .insert(connection_id);

        Ok(vec![])
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mut connections = self.connections.lock();

        connections
            .pending_outbound_connections
            .remove(&connection_id);

        if connections.reserved_peers.contains(&peer) {
            return Ok(dummy::ConnectionHandler)
        }

        check_limit(
            self.limits.max_established_outgoing,
            connections.established_outbound_connections.len(),
            Kind::EstablishedOutgoing,
        )?;
        check_limit(
            self.limits.max_established_per_peer,
            connections
                .established_per_peer
                .get(&peer)
                .map(|connections| connections.len())
                .unwrap_or(0),
            Kind::EstablishedPerPeer,
        )?;
        let total = connections
            .established_inbound_connections
            .len()
            .saturating_add(connections.established_outbound_connections.len());
        check_limit(
            self.limits.max_established_total,
            total,
            Kind::EstablishedTotal,
        )?;

        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let mut connections = self.connections.lock();
        match event {
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                remaining_established,
                ..
            }) => {
                connections
                    .established_inbound_connections
                    .remove(&connection_id);
                connections
                    .established_outbound_connections
                    .remove(&connection_id);

                if !connections.reserved_peers.contains(&peer_id) {
                    let entry = connections.established_per_peer.entry(peer_id);

                    match entry {
                        Entry::Occupied(mut entry) => {
                            entry.get_mut().remove(&connection_id);

                            if remaining_established == 0 || entry.get().is_empty() {
                                entry.remove();
                            }
                        }
                        Entry::Vacant(_) => {
                            // Do nothing
                        }
                    }
                }
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                endpoint,
                connection_id,
                ..
            }) => {
                match endpoint {
                    ConnectedPoint::Listener { .. } => {
                        connections
                            .established_inbound_connections
                            .insert(connection_id);
                    }
                    ConnectedPoint::Dialer { .. } => {
                        connections
                            .established_outbound_connections
                            .insert(connection_id);
                    }
                }

                if !connections.reserved_peers.contains(&peer_id) {
                    connections
                        .established_per_peer
                        .entry(peer_id)
                        .or_default()
                        .insert(connection_id);
                }
            }
            FromSwarm::DialFailure(DialFailure { connection_id, .. }) => {
                connections
                    .pending_outbound_connections
                    .remove(&connection_id);
            }
            FromSwarm::ListenFailure(ListenFailure { connection_id, .. }) => {
                connections
                    .pending_inbound_connections
                    .remove(&connection_id);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _id: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        void::unreachable(event)
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}
