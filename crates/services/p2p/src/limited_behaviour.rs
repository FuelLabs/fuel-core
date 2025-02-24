use crate::connection_limits::Connections;
use libp2p::{
    core::{
        transport::PortUse,
        Endpoint,
        Multiaddr,
    },
    identity::PeerId,
    swarm::{
        behaviour::ConnectionEstablished,
        derive_prelude::Either,
        dummy,
        AddressChange,
        ConnectionClosed,
        ConnectionDenied,
        ConnectionId,
        DialFailure,
        FromSwarm,
        ListenFailure,
        NetworkBehaviour,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
};
use std::{
    collections::HashSet,
    ops::{
        Deref,
        DerefMut,
    },
    task::{
        Context,
        Poll,
    },
};

pub struct LimitedBehaviour<Behaviour> {
    connection_limit: usize,
    behaviour: Behaviour,
    connections: Connections,
    connected_peers: HashSet<PeerId>,
    known_connections: HashSet<ConnectionId>,
}

impl<Behaviour> Deref for LimitedBehaviour<Behaviour> {
    type Target = Behaviour;

    fn deref(&self) -> &Self::Target {
        &self.behaviour
    }
}

impl<Behaviour> DerefMut for LimitedBehaviour<Behaviour> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.behaviour
    }
}

impl<Behaviour> LimitedBehaviour<Behaviour> {
    pub fn new(
        connection_limit: usize,
        connections: Connections,
        behaviour: Behaviour,
    ) -> Self {
        Self {
            connection_limit,
            behaviour,
            connections,
            connected_peers: Default::default(),
            known_connections: Default::default(),
        }
    }
}

impl<Behaviour> NetworkBehaviour for LimitedBehaviour<Behaviour>
where
    Behaviour: NetworkBehaviour,
{
    type ConnectionHandler =
        Either<dummy::ConnectionHandler, Behaviour::ConnectionHandler>;
    type ToSwarm = Behaviour::ToSwarm;

    fn handle_pending_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        Ok(())
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let connections = self.connections.lock();
        let total_connections = connections.established_per_peer.len();

        if connections.reserved_peers.contains(&peer)
            || self.connected_peers.contains(&peer)
            || total_connections < self.connection_limit
        {
            self.connected_peers.insert(peer);
            self.known_connections.insert(connection_id);
            let handler = self.behaviour.handle_established_inbound_connection(
                connection_id,
                peer,
                local_addr,
                remote_addr,
            )?;
            Ok(Either::Right(handler))
        } else {
            Ok(Either::Left(dummy::ConnectionHandler))
        }
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.behaviour.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let connections = self.connections.lock();
        let total_connections = connections.established_per_peer.len();

        if connections.reserved_peers.contains(&peer)
            || self.connected_peers.contains(&peer)
            || total_connections < self.connection_limit
        {
            self.connected_peers.insert(peer);
            self.known_connections.insert(connection_id);
            let handler = self.behaviour.handle_established_outbound_connection(
                connection_id,
                peer,
                addr,
                role_override,
                port_use,
            )?;
            Ok(Either::Right(handler))
        } else {
            Ok(Either::Left(dummy::ConnectionHandler))
        }
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let allowed = match &event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                connection_id,
                ..
            })
            | FromSwarm::AddressChange(AddressChange { connection_id, .. })
            | FromSwarm::DialFailure(DialFailure { connection_id, .. })
            | FromSwarm::ListenFailure(ListenFailure { connection_id, .. }) => {
                self.known_connections.contains(connection_id)
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                connection_id,
                peer_id,
                remaining_established,
                ..
            }) => {
                if *remaining_established == 0 {
                    self.connected_peers.remove(peer_id);
                }
                self.known_connections.remove(connection_id)
            }

            FromSwarm::NewListener(_)
            | FromSwarm::NewListenAddr(_)
            | FromSwarm::ExpiredListenAddr(_)
            | FromSwarm::ListenerError(_)
            | FromSwarm::ListenerClosed(_)
            | FromSwarm::NewExternalAddrCandidate(_)
            | FromSwarm::ExternalAddrConfirmed(_)
            | FromSwarm::ExternalAddrExpired(_)
            | FromSwarm::NewExternalAddrOfPeer(_) => true,
            _ => {
                tracing::warn!("Unhandled Swarm event in limited behaviour: {:?}", event);
                true
            }
        };

        if allowed {
            self.behaviour.on_swarm_event(event);
        }
    }

    fn on_connection_handler_event(
        &mut self,
        id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(event) => void::unreachable(event),
            Either::Right(event) => {
                self.behaviour
                    .on_connection_handler_event(id, connection_id, event)
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        match self.behaviour.poll(cx) {
            Poll::Ready(event) => {
                let ready = match event {
                    ToSwarm::GenerateEvent(event) => ToSwarm::GenerateEvent(event),
                    ToSwarm::Dial { opts } => ToSwarm::Dial { opts },
                    ToSwarm::ListenOn { opts } => ToSwarm::ListenOn { opts },
                    ToSwarm::RemoveListener { id } => ToSwarm::RemoveListener { id },
                    ToSwarm::NotifyHandler {
                        peer_id,
                        handler,
                        event,
                    } => ToSwarm::NotifyHandler {
                        peer_id,
                        handler,
                        event: Either::Right(event),
                    },
                    ToSwarm::NewExternalAddrCandidate(addr) => {
                        ToSwarm::NewExternalAddrCandidate(addr)
                    }
                    ToSwarm::ExternalAddrConfirmed(addr) => {
                        ToSwarm::ExternalAddrConfirmed(addr)
                    }
                    ToSwarm::ExternalAddrExpired(addr) => {
                        ToSwarm::ExternalAddrExpired(addr)
                    }
                    ToSwarm::CloseConnection {
                        peer_id,
                        connection,
                    } => ToSwarm::CloseConnection {
                        peer_id,
                        connection,
                    },
                    ToSwarm::NewExternalAddrOfPeer { peer_id, address } => {
                        ToSwarm::NewExternalAddrOfPeer { peer_id, address }
                    }
                    _ => {
                        tracing::warn!("Unhandled ToSwarm event in limited behaviour");
                        return Poll::Pending
                    }
                };

                Poll::Ready(ready)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
