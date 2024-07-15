use crate::{
    config::{
        Config,
        MAX_RESPONSE_SIZE,
    },
    TryPeerId,
};
use fuel_core_metrics::global_registry;
use libp2p::gossipsub::{
    self,
    MessageAuthenticity,
    MessageId,
    MetricsConfig,
    PeerScoreParams,
    PeerScoreThresholds,
    Topic,
    TopicScoreParams,
};
use sha2::{
    Digest,
    Sha256,
};
use std::{
    ops::DerefMut,
    time::Duration,
};

use super::topics::{
    GossipTopic,
    NEW_TX_GOSSIP_TOPIC,
};

// The number of slots in each epoch.
const SLOTS_PER_EPOCH: u64 = 32;

// The duration of each slot in seconds. This is the amount of time allotted for each opportunity to create a block.
const SLOT: Duration = Duration::from_secs(6);

// The total duration of an epoch in seconds, calculated as the number of slots per epoch times the duration of each slot.
const EPOCH: Duration = Duration::from_secs(SLOTS_PER_EPOCH * SLOT.as_secs());

// The factor by which scores decay towards zero in the scoring mechanism. Scores are reduced by this factor at each decay interval.
const DECAY_TO_ZERO: f64 = 0.8;

// The time interval at which scores decay, set equal to the duration of one slot.
const DECAY_INTERVAL: Duration = SLOT;

// The target number of peers in each gossip mesh.
const MESH_SIZE: usize = 8;

// The weight applied to the score for delivering new transactions.
const NEW_TX_GOSSIP_WEIGHT: f64 = 0.05;

// The threshold for a peer's score to be considered for greylisting.
// If a peer's score falls below this value, they will be greylisted.
// Greylisting is a lighter form of banning, where the peer's messages might be ignored or given lower priority,
// but the peer is not completely banned from the network.
pub const GRAYLIST_THRESHOLD: f64 = -16000.0;

/// Creates `GossipsubConfigBuilder` with few of the Gossipsub values already defined
pub fn default_gossipsub_builder() -> gossipsub::ConfigBuilder {
    let gossip_message_id = move |message: &gossipsub::Message| {
        MessageId::from(&Sha256::digest(&message.data)[..])
    };

    let mut builder = gossipsub::ConfigBuilder::default();

    builder
        .protocol_id_prefix("/meshsub/1.0.0")
        .message_id_fn(gossip_message_id)
        .validate_messages();

    builder
}

/// Builds a default `GossipsubConfig`.
/// Used in testing.
pub(crate) fn default_gossipsub_config() -> gossipsub::Config {
    default_gossipsub_builder()
        .mesh_n(MESH_SIZE)
        .mesh_n_low(6)
        .mesh_n_high(12)
        .gossip_lazy(6)
        .history_length(5)
        .history_gossip(3)
        .max_transmit_size(MAX_RESPONSE_SIZE)
        .heartbeat_interval(Duration::from_millis(700))
        .fanout_ttl(Duration::from_secs(60))
        .build()
        .expect("valid gossipsub configuration")
}

fn initialize_topic_score_params(topic_weight: f64) -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = topic_weight;

    // The "quantum" of time spent in the mesh, set to the duration of a slot.
    // This is the smallest unit of time for which we track a peer's presence in the mesh.
    params.time_in_mesh_quantum = SLOT;
    params.time_in_mesh_cap = 3600.0 / params.time_in_mesh_quantum.as_secs_f64();
    params.time_in_mesh_weight = 0.5;

    // The decay time for the first message delivered score, set to 100 times the epoch duration.
    // This means that the score given for first message deliveries will decay over this time period.
    params.first_message_deliveries_decay = score_parameter_decay(
        EPOCH
            .checked_mul(100)
            .expect("`EPOCH` is usually not more than a year"),
    );
    params.first_message_deliveries_cap = 1000.0;
    params.first_message_deliveries_weight = 0.5;

    params.mesh_message_deliveries_weight = 0.0;
    params.mesh_message_deliveries_threshold = 0.0;
    params.mesh_message_deliveries_decay = 0.0;
    params.mesh_message_deliveries_cap = 0.0;
    params.mesh_message_deliveries_window = Duration::from_secs(0);
    params.mesh_message_deliveries_activation = Duration::from_secs(0);
    params.mesh_failure_penalty_decay = 0.0;
    params.mesh_failure_penalty_weight = 0.0;

    params.invalid_message_deliveries_weight = -10.0 / params.topic_weight; // -200 per invalid message
    params.invalid_message_deliveries_decay = score_parameter_decay(
        EPOCH
            .checked_mul(50)
            .expect("`EPOCH` is usually not more than a year"),
    );

    params
}

fn score_parameter_decay(decay_time: Duration) -> f64 {
    let ticks = decay_time.as_secs_f64() / DECAY_INTERVAL.as_secs_f64();
    DECAY_TO_ZERO.powf(1.0 / ticks)
}

fn initialize_peer_score_params(thresholds: &PeerScoreThresholds) -> PeerScoreParams {
    let mut params = PeerScoreParams {
        decay_interval: DECAY_INTERVAL,
        decay_to_zero: DECAY_TO_ZERO,
        retain_score: EPOCH
            .checked_mul(100)
            .expect("`EPOCH` is usually not more than a year"),
        app_specific_weight: 0.0,
        ip_colocation_factor_threshold: 50.0, // Allow up to 50 nodes per IP
        behaviour_penalty_threshold: 6.0,
        behaviour_penalty_decay: score_parameter_decay(
            EPOCH
                .checked_mul(10)
                .expect("`EPOCH` is usually not more than a year"),
        ),
        ..Default::default()
    };

    let target_value =
        params.behaviour_penalty_decay - params.behaviour_penalty_threshold;

    params.behaviour_penalty_weight = thresholds.gossip_threshold / target_value.powi(2);

    params.topic_score_cap = 400.0;
    params.ip_colocation_factor_weight = -params.topic_score_cap;

    params
}

fn initialize_peer_score_thresholds() -> PeerScoreThresholds {
    PeerScoreThresholds {
        gossip_threshold: -4000.0,
        publish_threshold: -8000.0,
        graylist_threshold: GRAYLIST_THRESHOLD,
        accept_px_threshold: 40.0,
        opportunistic_graft_threshold: 5.0,
    }
}

/// Given a `P2pConfig` containing `GossipsubConfig` creates a Gossipsub Behaviour
pub(crate) fn build_gossipsub_behaviour(p2p_config: &Config) -> gossipsub::Behaviour {
    let mut gossipsub = if p2p_config.metrics {
        let mut registry = global_registry().registry.lock();

        let metrics_config = MetricsConfig::default();

        let mut gossipsub = gossipsub::Behaviour::new_with_metrics(
            MessageAuthenticity::Signed(p2p_config.keypair.clone()),
            p2p_config.gossipsub_config.clone(),
            registry.deref_mut(),
            metrics_config,
        )
        .expect("gossipsub initialized");

        initialize_gossipsub(&mut gossipsub, p2p_config);

        gossipsub
    } else {
        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(p2p_config.keypair.clone()),
            p2p_config.gossipsub_config.clone(),
        )
        .expect("gossipsub initialized");

        initialize_gossipsub(&mut gossipsub, p2p_config);

        gossipsub
    };
    let reserved_nodes = p2p_config.reserved_nodes.clone();
    let explicit_peers = reserved_nodes
        .iter()
        .filter_map(|address| address.try_to_peer_id());
    for peer_id in explicit_peers {
        gossipsub.add_explicit_peer(&peer_id);
    }

    gossipsub
}

fn initialize_gossipsub(gossipsub: &mut gossipsub::Behaviour, p2p_config: &Config) {
    let peer_score_thresholds = initialize_peer_score_thresholds();
    let peer_score_params = initialize_peer_score_params(&peer_score_thresholds);

    gossipsub
        .with_peer_score(peer_score_params, peer_score_thresholds)
        .expect("gossipsub initialized with peer score");

    let topics = vec![(NEW_TX_GOSSIP_TOPIC, NEW_TX_GOSSIP_WEIGHT)];

    // subscribe to gossipsub topics with the network name suffix
    for (topic, weight) in topics {
        let t: GossipTopic = Topic::new(format!("{}/{}", topic, p2p_config.network_name));

        gossipsub
            .set_topic_params(t.clone(), initialize_topic_score_params(weight))
            .expect("First time initializing Topic Score");

        gossipsub
            .subscribe(&t)
            .expect("Subscription to Topic: {topic} successful");
    }
}
