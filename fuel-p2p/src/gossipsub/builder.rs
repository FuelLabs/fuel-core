use fuel_metrics::p2p_metrics::P2P_METRICS;
use libp2p::{
    gossipsub::{
        metrics::Config as MetricsConfig,
        FastMessageId,
        Gossipsub,
        GossipsubConfigBuilder,
        GossipsubMessage,
        MessageAuthenticity,
        MessageId,
        PeerScoreParams,
        PeerScoreThresholds,
        RawGossipsubMessage,
    },
    identity::Keypair,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::config::P2PConfig;

pub fn build_gossipsub(local_key: &Keypair, p2p_config: &P2PConfig) -> Gossipsub {
    let gossip_message_id = move |message: &GossipsubMessage| {
        MessageId::from(&Sha256::digest(&message.data)[..20])
    };

    let fast_gossip_message_id = move |message: &RawGossipsubMessage| {
        FastMessageId::from(&Sha256::digest(&message.data)[..8])
    };

    let gossipsub_config = GossipsubConfigBuilder::default()
        .protocol_id_prefix("/meshsub/1.0.0")
        .mesh_n(p2p_config.ideal_mesh_size)
        .mesh_n_low(p2p_config.min_mesh_size)
        .mesh_n_high(p2p_config.max_mesh_size)
        .message_id_fn(gossip_message_id)
        .fast_message_id_fn(fast_gossip_message_id)
        .build()
        .expect("valid gossipsub configuration");

    // Move to Metrics related feature flag
    let p2p_registry = &mut P2P_METRICS
        .write()
        .expect("Something already captured p2p metrics")
        .gossip_sub_registry;
    let metrics_config = MetricsConfig::default();

    let mut gossipsub = Gossipsub::new_with_metrics(
        MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
        p2p_registry,
        metrics_config,
    )
    .expect("gossipsub initialized");

    gossipsub
        .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
        .expect("gossipsub initialized with peer score");

    gossipsub
}
