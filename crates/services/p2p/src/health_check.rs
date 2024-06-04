use libp2p::{
    core::Endpoint,
    swarm::{
        derive_prelude::FromSwarm,
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

#[derive(Debug, Clone)]
pub enum HealthCheckEvent {
    /// Informs p2p service / PeerManager to check health of reserved nodes' connections
    CheckReservedNodesHealth,
}

pub struct Behavior {
    health_check: Interval,
}

impl Behavior {
    pub fn new() -> Self {
        Self {
            health_check: time::interval(Duration::from_secs(
                HEALTH_CHECK_INTERVAL_IN_SECONDS,
            )),
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
    type ToSwarm = HealthCheckEvent;

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

    fn on_swarm_event(&mut self, _event: FromSwarm) {}

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
        if self.health_check.poll_tick(cx).is_ready() {
            return Poll::Ready(ToSwarm::GenerateEvent(
                HealthCheckEvent::CheckReservedNodesHealth,
            ));
        }

        Poll::Pending
    }
}
