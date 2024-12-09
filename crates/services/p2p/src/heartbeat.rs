use crate::Multiaddr;
use fuel_core_types::fuel_types::BlockHeight;
pub use handler::Config;
use handler::{
    HeartbeatHandler,
    HeartbeatInEvent,
    HeartbeatOutEvent,
};
use libp2p::{
    core::{
        transport::PortUse,
        Endpoint,
    },
    swarm::{
        derive_prelude::ConnectionId,
        ConnectionDenied,
        FromSwarm,
        NetworkBehaviour,
        NotifyHandler,
        THandler,
        THandlerInEvent,
        THandlerOutEvent,
        ToSwarm,
    },
    PeerId,
};
use std::{
    collections::VecDeque,
    task::Poll,
};

mod handler;

pub const HEARTBEAT_PROTOCOL: &str = "/fuel/heartbeat/0.0.1";

#[derive(Debug, Clone)]
enum HeartbeatAction {
    HeartbeatEvent(Event),
    BlockHeightRequest {
        peer_id: PeerId,
        connection_id: ConnectionId,
        in_event: HeartbeatInEvent,
    },
}

impl HeartbeatAction {
    fn build(self) -> ToSwarm<Event, HeartbeatInEvent> {
        match self {
            Self::HeartbeatEvent(event) => ToSwarm::GenerateEvent(event),
            Self::BlockHeightRequest {
                peer_id,
                connection_id,
                in_event,
            } => ToSwarm::NotifyHandler {
                handler: NotifyHandler::One(connection_id),
                peer_id,
                event: in_event,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Event {
    pub peer_id: PeerId,
    pub latest_block_height: BlockHeight,
}

#[derive(Debug, Clone)]
pub struct Behaviour {
    config: Config,
    pending_events: VecDeque<HeartbeatAction>,
    current_block_height: BlockHeight,
}

impl Behaviour {
    pub fn new(config: Config, block_height: BlockHeight) -> Self {
        Self {
            config,
            pending_events: VecDeque::default(),
            current_block_height: block_height,
        }
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.current_block_height = block_height;
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = HeartbeatHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(HeartbeatHandler::new(self.config.clone()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(HeartbeatHandler::new(self.config.clone()))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HeartbeatOutEvent::BlockHeight(latest_block_height) => self
                .pending_events
                .push_back(HeartbeatAction::HeartbeatEvent(Event {
                    peer_id,
                    latest_block_height,
                })),
            HeartbeatOutEvent::RequestBlockHeight => {
                self.pending_events
                    .push_back(HeartbeatAction::BlockHeightRequest {
                        peer_id,
                        connection_id,
                        in_event: HeartbeatInEvent::LatestBlock(
                            self.current_block_height,
                        ),
                    })
            }
        }
    }

    fn poll(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(action) = self.pending_events.pop_front() {
            return Poll::Ready(action.build());
        }

        Poll::Pending
    }
}
