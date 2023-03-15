use crate::config::{
    Config,
    MAX_RESPONSE_SIZE,
};
use fuel_core_metrics::p2p_metrics::P2P_METRICS;
use libp2p::gossipsub::{
    metrics::Config as MetricsConfig,
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
use prometheus_client::registry::Registry;
use sha2::{
    Digest,
    Sha256,
};
use std::time::Duration;

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
        .max_transmit_size(MAX_RESPONSE_SIZE)
        .heartbeat_interval(Duration::from_secs(1))
        .build()
        .expect("valid gossipsub configuration")
}

/// Given a `P2pConfig` containing `GossipsubConfig` creates a Gossipsub Behaviour
pub(crate) fn build_gossipsub_behaviour(p2p_config: &Config) -> Gossipsub {
    if p2p_config.metrics {
        // Move to Metrics related feature flag
        let mut p2p_registry = Registry::default();

        let metrics_config = MetricsConfig::default();

        let mut gossipsub = Gossipsub::new_with_metrics(
            MessageAuthenticity::Signed(p2p_config.keypair.clone()),
            p2p_config.gossipsub_config.clone(),
            &mut p2p_registry,
            metrics_config,
        )
        .expect("gossipsub initialized");

        // This couldn't be set unless multiple p2p services are running? So it's ok to unwrap
        P2P_METRICS
            .gossip_sub_registry
            .set(Box::new(p2p_registry))
            .unwrap_or(());

        gossipsub
            .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
            .expect("gossipsub initialized with peer score");

        gossipsub
    } else {
        let mut gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(p2p_config.keypair.clone()),
            p2p_config.gossipsub_config.clone(),
        )
        .expect("gossipsub initialized");

        gossipsub
            .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
            .expect("gossipsub initialized with peer score");

        gossipsub
    }
}
