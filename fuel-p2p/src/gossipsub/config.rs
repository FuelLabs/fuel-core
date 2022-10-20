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

/// Creates `GossipsubConfigBuilder` with few of the Gossipsub values already defined
pub fn default_gossipsub_builder() -> GossipsubConfigBuilder {
    let gossip_message_id = move |message: &GossipsubMessage| {
        MessageId::from(&Sha256::digest(&message.data)[..])
    };

    let fast_gossip_message_id = move |message: &RawGossipsubMessage| {
        FastMessageId::from(&Sha256::digest(&message.data)[..])
    };

    let mut builder = GossipsubConfigBuilder::default();

    builder
        .protocol_id_prefix("/meshsub/1.0.0")
        .message_id_fn(gossip_message_id)
        .fast_message_id_fn(fast_gossip_message_id)
        .validate_messages();

    builder
}

/// Builds a default `GossipsubConfig`.
/// Used in testing.
pub(crate) fn default_gossipsub_config() -> GossipsubConfig {
    default_gossipsub_builder()
        .mesh_n(6)
        .mesh_n_low(4)
        .mesh_n_high(12)
        .build()
        .expect("valid gossipsub configuration")
}

/// Given a `P2pConfig` creates a Gossipsub Behaviour
pub(crate) fn build_gossipsub_behaviour(p2p_config: &P2PConfig) -> Gossipsub {
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
