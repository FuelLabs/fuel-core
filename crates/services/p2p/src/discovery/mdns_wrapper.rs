use crate::Multiaddr;
use libp2p::{
    core::{
        transport::PortUse,
        Endpoint,
    },
    mdns::{
        self,
        tokio::Behaviour as TokioMdns,
    },
    swarm::{
        dummy,
        ConnectionDenied,
        ConnectionId,
        FromSwarm,
        NetworkBehaviour,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
    PeerId,
};
use std::task::{
    Context,
    Poll,
};
use tracing::warn;

#[allow(clippy::large_enum_variant)]
pub enum MdnsWrapper {
    Ready(TokioMdns),
    Disabled,
}

impl MdnsWrapper {
    pub fn new(peer_id: PeerId) -> Self {
        match TokioMdns::new(mdns::Config::default(), peer_id) {
            Ok(mdns) => Self::Ready(mdns),
            Err(err) => {
                warn!("Failed to initialize mDNS: {:?}", err);
                Self::Disabled
            }
        }
    }

    pub fn disabled() -> Self {
        MdnsWrapper::Disabled
    }

    pub fn on_swarm_event(&mut self, event: &FromSwarm) {
        match self {
            MdnsWrapper::Ready(mdns) => match event {
                FromSwarm::NewListenAddr(event) => {
                    mdns.on_swarm_event(FromSwarm::NewListenAddr(*event))
                }
                FromSwarm::ExpiredListenAddr(event) => {
                    mdns.on_swarm_event(FromSwarm::ExpiredListenAddr(*event))
                }
                _ => {}
            },
            MdnsWrapper::Disabled => {}
        }
    }
}

impl NetworkBehaviour for MdnsWrapper {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = mdns::Event;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        match self {
            MdnsWrapper::Ready(mdns) => mdns.handle_established_inbound_connection(
                connection_id,
                peer,
                local_addr,
                remote_addr,
            ),
            MdnsWrapper::Disabled => Ok(dummy::ConnectionHandler),
        }
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        match self {
            MdnsWrapper::Ready(mdns) => mdns.handle_pending_outbound_connection(
                connection_id,
                maybe_peer,
                addresses,
                effective_role,
            ),
            MdnsWrapper::Disabled => Ok(vec![]),
        }
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        match self {
            MdnsWrapper::Ready(mdns) => mdns.handle_established_outbound_connection(
                connection_id,
                peer,
                addr,
                role_override,
                port_use,
            ),
            MdnsWrapper::Disabled => Ok(dummy::ConnectionHandler),
        }
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.on_swarm_event(&event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match self {
            MdnsWrapper::Ready(mdns) => {
                mdns.on_connection_handler_event(peer_id, connection, event)
            }
            MdnsWrapper::Disabled => {}
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        match self {
            MdnsWrapper::Ready(mdns) => mdns.poll(cx),
            MdnsWrapper::Disabled => Poll::Pending,
        }
    }
}
