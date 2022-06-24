use libp2p::gossipsub::{Sha256Topic, Topic, TopicHash};

use super::messages::{GossipTopicTag, GossipsubBroadcastRequest};

pub type GossipTopic = Sha256Topic;
pub const NEW_TX_GOSSIP_TOPIC: &str = "new_tx";
pub const NEW_BLOCK_GOSSIP_TOPIC: &str = "new_block";
pub const CON_VOTE_GOSSIP_TOPIC: &str = "consensus_vote";

/// Holds used Gossipsub Topics
/// Each field contains TopicHash and GossipTopic itslef
/// in order to aviod converting GossipTopic to TopicHash on each received message
#[derive(Debug)]
pub struct GossipsubTopics {
    new_tx_topic: (TopicHash, GossipTopic),
    new_block_topic: (TopicHash, GossipTopic),
    consensus_vote_topic: (TopicHash, GossipTopic),
}

impl GossipsubTopics {
    pub fn new(network_name: &str) -> Self {
        let new_tx_topic = Topic::new(format!("{}/{}", NEW_TX_GOSSIP_TOPIC, network_name));
        let new_block_topic = Topic::new(format!("{}/{}", NEW_BLOCK_GOSSIP_TOPIC, network_name));
        let consensus_vote_topic =
            Topic::new(format!("{}/{}", CON_VOTE_GOSSIP_TOPIC, network_name));

        Self {
            new_tx_topic: (new_tx_topic.hash(), new_tx_topic),
            new_block_topic: (new_block_topic.hash(), new_block_topic),
            consensus_vote_topic: (consensus_vote_topic.hash(), consensus_vote_topic),
        }
    }

    /// Given a TopicHash it will return a matching GossipTopicTag
    pub fn get_gossipsub_tag(&self, incoming_topic: &TopicHash) -> Option<GossipTopicTag> {
        let GossipsubTopics {
            new_tx_topic,
            new_block_topic,
            consensus_vote_topic,
        } = &self;

        match incoming_topic {
            hash if hash == &new_tx_topic.0 => Some(GossipTopicTag::NewTx),
            hash if hash == &new_block_topic.0 => Some(GossipTopicTag::NewBlock),
            hash if hash == &consensus_vote_topic.0 => Some(GossipTopicTag::ConensusVote),
            _ => None,
        }
    }

    /// Given a `GossipsubBroadcastRequest` retruns a `GossipTopic`
    /// which is broadcast over the network with the serialized inner value of `GossipsubBroadcastRequest`
    pub fn get_gossipsub_topic(&self, outgoing_request: &GossipsubBroadcastRequest) -> GossipTopic {
        match outgoing_request {
            GossipsubBroadcastRequest::ConensusVote(_) => self.consensus_vote_topic.1.clone(),
            GossipsubBroadcastRequest::NewBlock(_) => self.new_block_topic.1.clone(),
            GossipsubBroadcastRequest::NewTx(_) => self.new_tx_topic.1.clone(),
        }
    }
}
