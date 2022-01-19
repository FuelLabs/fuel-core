use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

use libp2p::{
    gossipsub::{Gossipsub, GossipsubEvent, TopicHash},
    identify::IdentifyEvent,
    identity::Keypair,
    swarm::{
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
    },
    NetworkBehaviour, PeerId,
};

use crate::discovery::{Discovery, DiscoveryOutEvent};

#[derive(Debug)]
pub enum FuelBehaviourEvent {
    GossipsubMessage {
        peer_id: PeerId,
        topic: TopicHash,
        message: Vec<u8>,
    },
}

#[derive(NetworkBehaviour)]
#[behaviour(
    out_event = "FuelBehaviourEvent",
    poll_method = "poll",
    event_process = true
)]
pub struct FuelBehaviour {
    /// message propagation for p2p
    gossipsub: Gossipsub,

    /// node discovery
    discovery: Discovery,

    //peer info let's do it later
    //identify: Identify,
    #[behaviour(ignore)]
    events: VecDeque<FuelBehaviourEvent>,
}

impl FuelBehaviour {
    pub fn new(local_key: Keypair) -> Self {
        todo!()
    }

    fn poll(
        &mut self,
        _cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self as NetworkBehaviour>::OutEvent,
            <Self as NetworkBehaviour>::ProtocolsHandler,
        >,
    > {
        match self.events.pop_front() {
            Some(event) => Poll::Ready(NetworkBehaviourAction::GenerateEvent(event)),
            _ => Poll::Pending,
        }
    }
}

impl NetworkBehaviourEventProcess<GossipsubEvent> for FuelBehaviour {
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message {
            propagation_source,
            message,
            message_id: _,
        } = message
        {
            self.events.push_back(FuelBehaviourEvent::GossipsubMessage {
                peer_id: propagation_source,
                topic: message.topic,
                message: message.data,
            })
        }
    }
}

impl NetworkBehaviourEventProcess<DiscoveryOutEvent> for FuelBehaviour {
    fn inject_event(&mut self, _event: DiscoveryOutEvent) {
        todo!()
    }
}

impl NetworkBehaviourEventProcess<IdentifyEvent> for FuelBehaviour {
    fn inject_event(&mut self, _event: IdentifyEvent) {
        todo!()
    }
}
