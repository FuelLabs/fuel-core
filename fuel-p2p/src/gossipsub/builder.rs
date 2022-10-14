use libp2p::gossipsub::{
    FastMessageId,
    Gossipsub,
    GossipsubConfig,
    GossipsubConfigBuilder,
    GossipsubMessage,
    MessageAuthenticity,
    MessageId,
    PeerScoreParams,
    PeerScoreThresholds,
    RawGossipsubMessage,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::config::P2PConfig;

pub fn default_gossipsub_config() -> GossipsubConfig {
    let gossip_message_id = move |message: &GossipsubMessage| {
        MessageId::from(&Sha256::digest(&message.data)[..])
    };

    let fast_gossip_message_id = move |message: &RawGossipsubMessage| {
        FastMessageId::from(&Sha256::digest(&message.data)[..])
    };

    let gossipsub_config = GossipsubConfigBuilder::default()
        .protocol_id_prefix("/meshsub/1.0.0")
        .mesh_n(6)
        .mesh_n_low(4)
        .mesh_n_high(12)
        .message_id_fn(gossip_message_id)
        .fast_message_id_fn(fast_gossip_message_id)
        .validate_messages()
        .build()
        .expect("valid gossipsub configuration");

    gossipsub_config
}

pub fn build_gossipsub(p2p_config: &P2PConfig) -> Gossipsub {
    let mut gossipsub = Gossipsub::new(
        MessageAuthenticity::Signed(p2p_config.local_keypair.clone()),
        p2p_config.gossipsub_config.clone(),
    )
    .expect("gossipsub initialized");

    gossipsub
        .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
        .expect("gossipsub initialized with peer score");

    gossipsub
}
