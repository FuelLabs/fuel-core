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
    Topic,
    TopicScoreParams,
};

use prometheus_client::registry::Registry;
use sha2::{
    Digest,
    Sha256,
};
use std::time::Duration;

use super::topics::GossipTopic;

const MAX_GOSSIP_SCORE: f64 = 100.0;

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

fn initialize_topic_score_params() -> TopicScoreParams {
    TopicScoreParams {
        /// Each topic will weigh this much
        topic_weight: 0.5,

        // Reflects positive on the Score

        //  Time in the mesh
        //  This is the time the peer has been grafted in the mesh.
        //  The value of of the parameter is the `time/time_in_mesh_quantum`, capped by `time_in_mesh_cap`
        //  The weight of the parameter must be positive (or zero to disable).
        time_in_mesh_weight: 1.0,
        time_in_mesh_quantum: Duration::from_millis(1),
        time_in_mesh_cap: 50.0,

        ///  First message deliveries
        ///  This is the number of message deliveries in the topic.
        ///  The value of the parameter is a counter, decaying with `first_message_deliveries_decay`, and capped
        ///  by `first_message_deliveries_cap`.
        ///  The weight of the parameter MUST be positive (or zero to disable).
        first_message_deliveries_weight: 1.0,
        first_message_deliveries_decay: 0.5,
        first_message_deliveries_cap: 50.0,

        // Reflects negative on the Score
        ///  Mesh message deliveries
        ///  This is the number of message deliveries in the mesh, within the
        ///  `mesh_message_deliveries_window` of message validation; deliveries during validation also
        ///  count and are retroactively applied when validation succeeds.
        ///  This window accounts for the minimum time before a hostile mesh peer trying to game the
        ///  score could replay back a valid message we just sent them.
        ///  It effectively tracks first and near-first deliveries, ie a message seen from a mesh peer
        ///  before we have forwarded it to them.
        ///  The parameter has an associated counter, decaying with `mesh_message_deliveries_decay`.
        ///  If the counter exceeds the threshold, its value is 0.
        ///  If the counter is below the `mesh_message_deliveries_threshold`, the value is the square of
        ///  the deficit, ie (`message_deliveries_threshold - counter)^2`
        ///  The penalty is only activated after `mesh_message_deliveries_activation` time in the mesh.
        ///  The weight of the parameter MUST be negative (or zero to disable).
        mesh_message_deliveries_weight: -1.0,
        mesh_message_deliveries_decay: 0.5,
        mesh_message_deliveries_cap: 10.0,
        mesh_message_deliveries_threshold: 20.0,
        mesh_message_deliveries_window: Duration::from_millis(10),
        mesh_message_deliveries_activation: Duration::from_secs(5),

        ///  Sticky mesh propagation failures
        ///  This is a sticky penalty that applies when a peer gets pruned from the mesh with an active
        ///  mesh message delivery penalty.
        ///  The weight of the parameter MUST be negative (or zero to disable)
        mesh_failure_penalty_weight: -1.0,
        mesh_failure_penalty_decay: 0.5,

        ///  Invalid messages
        ///  This is the number of invalid messages in the topic.
        ///  The value of the parameter is the square of the counter, decaying with
        ///  `invalid_message_deliveries_decay`.
        ///  The weight of the parameter MUST be negative (or zero to disable).
        invalid_message_deliveries_weight: -1.0,
        invalid_message_deliveries_decay: 0.3,
    }
}

/// This function takes in `max_gossipsub_score` and sets it to `topic_score_cap`
/// The reasoning is because app-specific is set to 0 so the max peer score
/// can be the score of the topic score cap
fn initialize_peer_score_params() -> PeerScoreParams {
    PeerScoreParams {
        // topics are added later
        topics: Default::default(),

        /// Aggregate topic score cap; this limits the total contribution of topics towards a positive
        /// score. It must be positive (or 0 for no cap).
        topic_score_cap: MAX_GOSSIP_SCORE,

        // Application-specific peer scoring
        // We keep app score separate from gossipsub score
        app_specific_weight: 0.0,

        ///  IP-colocation factor.
        ///  The parameter has an associated counter which counts the number of peers with the same IP.
        ///  If the number of peers in the same IP exceeds `ip_colocation_factor_threshold, then the value
        ///  is the square of the difference, ie `(peers_in_same_ip - ip_colocation_threshold)^2`.
        ///  If the number of peers in the same IP is less than the threshold, then the value is 0.
        ///  The weight of the parameter MUST be negative, unless you want to disable for testing.            
        ip_colocation_factor_weight: -5.0,
        ip_colocation_factor_threshold: 10.0,
        ip_colocation_factor_whitelist: Default::default(),

        ///  Behavioural pattern penalties.
        ///  This parameter has an associated counter which tracks misbehaviour as detected by the
        ///  router. The router currently applies penalties for the following behaviors:
        ///  - attempting to re-graft before the prune backoff time has elapsed.
        ///  - not following up in IWANT requests for messages advertised with IHAVE.
        ///
        ///  The value of the parameter is the square of the counter over the threshold, which decays
        ///  with BehaviourPenaltyDecay.
        ///  The weight of the parameter MUST be negative (or zero to disable).
        behaviour_penalty_weight: -10.0,
        behaviour_penalty_threshold: 0.0,
        behaviour_penalty_decay: 0.2,

        /// The decay interval for parameter counters.
        decay_interval: Duration::from_secs(1),

        /// Counter value below which it is considered 0.
        decay_to_zero: 0.1,

        /// Time to remember counters for a disconnected peer.
        retain_score: Duration::from_secs(3600),
    }
}

fn initialize_peer_score_thresholds() -> PeerScoreThresholds {
    PeerScoreThresholds {
        /// The score threshold below which gossip propagation is suppressed;
        /// should be negative.
        gossip_threshold: -10.0,

        /// The score threshold below which we shouldn't publish when using flood
        /// publishing (also applies to fanout peers); should be negative and <= `gossip_threshold`.
        publish_threshold: -50.0,

        /// The score threshold below which message processing is suppressed altogether,
        /// implementing an effective graylist according to peer score; should be negative and
        /// <= `publish_threshold`.
        graylist_threshold: -80.0,

        /// The score threshold below which px will be ignored; this should be positive
        /// and limited to scores attainable by bootstrappers and other trusted nodes.
        accept_px_threshold: 10.0,

        /// The median mesh score threshold before triggering opportunistic
        /// grafting; this should have a small positive value.
        opportunistic_graft_threshold: 20.0,
    }
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

        initialize_gossipsub(&mut gossipsub, p2p_config);

        gossipsub
    } else {
        let mut gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(p2p_config.keypair.clone()),
            p2p_config.gossipsub_config.clone(),
        )
        .expect("gossipsub initialized");

        initialize_gossipsub(&mut gossipsub, p2p_config);

        gossipsub
    }
}

fn initialize_gossipsub(gossipsub: &mut Gossipsub, p2p_config: &Config) {
    let peer_score_params = initialize_peer_score_params();

    let peer_score_thresholds = initialize_peer_score_thresholds();

    let topic_score_params = initialize_topic_score_params();

    gossipsub
        .with_peer_score(peer_score_params, peer_score_thresholds)
        .expect("gossipsub initialized with peer score");

    // subscribe to gossipsub topics with the network name suffix
    for topic in &p2p_config.topics {
        let t: GossipTopic = Topic::new(format!("{}/{}", topic, p2p_config.network_name));

        gossipsub
            .set_topic_params(t.clone(), topic_score_params.clone())
            .expect("First time initializing Topic Score");

        gossipsub
            .subscribe(&t)
            .expect("Subscription to Topic: {topic} successful");
    }
}
