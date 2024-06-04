use libp2p::{
    self,
    core::Endpoint,
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
};

#[derive(Debug, Clone)]
pub enum PeerConnectionEvent {
    PeerConnected {
        peer_id: PeerId,
        initial_connection: bool,
    },
    PeerDisconnected {
        peer_id: PeerId,
    },
}

pub struct Behavior {
    pending_events: VecDeque<PeerConnectionEvent>,
}

impl Behavior {
    pub fn new() -> Self {
        Self {
            pending_events: VecDeque::default(),
        }
    }
}

impl Default for Behavior {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkBehaviour for Behavior {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = PeerConnectionEvent;

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
                    .push_back(PeerConnectionEvent::PeerConnected {
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
                        .push_back(PeerConnectionEvent::PeerDisconnected { peer_id })
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
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        Poll::Pending
    }
}
