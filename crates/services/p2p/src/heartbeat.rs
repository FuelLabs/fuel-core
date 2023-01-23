use fuel_core_types::blockchain::primitives::BlockHeight;
pub use handler::HeartbeatConfig;
use handler::{
    HeartbeatHandler,
    HeartbeatInEvent,
    HeartbeatOutEvent,
};
use libp2p::PeerId;
use libp2p_swarm::{
    derive_prelude::ConnectionId,
    ConnectionHandler,
    IntoConnectionHandler,
    NetworkBehaviour,
    NetworkBehaviourAction,
    NotifyHandler,
    PollParameters,
};
use std::{
    collections::VecDeque,
    task::Poll,
};
mod handler;

pub const HEARTBEAT_PROTOCOL: &[u8] = b"/fuel/heartbeat/0.0.1";

#[derive(Debug, Clone)]
enum HeartbeatAction {
    HeartbeatEvent(HeartbeatEvent),
    BlockHeightRequest {
        peer_id: PeerId,
        connection_id: ConnectionId,
        in_event: HeartbeatInEvent,
    },
}

impl HeartbeatAction {
    fn build(self) -> NetworkBehaviourAction<HeartbeatEvent, HeartbeatHandler> {
        match self {
            Self::HeartbeatEvent(event) => NetworkBehaviourAction::GenerateEvent(event),
            Self::BlockHeightRequest {
                peer_id,
                connection_id,
                in_event,
            } => NetworkBehaviourAction::NotifyHandler {
                handler: NotifyHandler::One(connection_id),
                peer_id,
                event: in_event,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HeartbeatEvent {
    pub peer_id: PeerId,
    pub latest_block_height: BlockHeight,
}

#[derive(Debug, Clone)]
pub struct Heartbeat {
    config: HeartbeatConfig,
    pending_events: VecDeque<HeartbeatAction>,
    current_block_height: BlockHeight,
}

impl Heartbeat {
    pub fn new(config: HeartbeatConfig, block_height: BlockHeight) -> Self {
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

impl NetworkBehaviour for Heartbeat {
    type ConnectionHandler = HeartbeatHandler;
    type OutEvent = HeartbeatEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        HeartbeatHandler::new(self.config.clone())
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <<Self::ConnectionHandler as IntoConnectionHandler>::Handler as
            ConnectionHandler>::OutEvent,
    ) {
        match event {
            HeartbeatOutEvent::BlockHeight(latest_block_height) => self
                .pending_events
                .push_back(HeartbeatAction::HeartbeatEvent(HeartbeatEvent {
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
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(action) = self.pending_events.pop_front() {
            return Poll::Ready(action.build())
        }

        Poll::Pending
    }
}
