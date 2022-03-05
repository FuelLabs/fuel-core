use libp2p::{
    gossipsub::{
        FastMessageId, Gossipsub, GossipsubConfigBuilder, GossipsubMessage, MessageAuthenticity,
        MessageId, PeerScoreParams, PeerScoreThresholds, RawGossipsubMessage, ValidationMode,
    },
    identity::Keypair,
};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub fn build_gossipsub(local_key: &Keypair) -> Gossipsub {
    let gossip_message_id =
        move |message: &GossipsubMessage| MessageId::from(&Sha256::digest(&message.data)[..20]);

    let fast_gossip_message_id = move |message: &RawGossipsubMessage| {
        FastMessageId::from(&Sha256::digest(&message.data)[..8])
    };

    let gossipsub_config = GossipsubConfigBuilder::default()
        .protocol_id_prefix("/meshsub/1.0.0")
        .retain_scores(4)
        .max_transmit_size(2048)
        .heartbeat_interval(Duration::from_secs(1))
        .heartbeat_initial_delay(Duration::from_secs(5))
        .check_explicit_peers_ticks(300)
        .idle_timeout(Duration::from_secs(120))
        .prune_peers(16)
        .prune_backoff(Duration::from_secs(60))
        .backoff_slack(1)
        .flood_publish(true)
        .mesh_n(6)
        .mesh_n_low(4)
        .mesh_n_high(12)
        .mesh_outbound_min(2)
        .gossip_lazy(6)
        .gossip_factor(0.25)
        .fanout_ttl(Duration::from_secs(60))
        .history_length(5)
        .gossip_retransimission(3)
        .max_messages_per_rpc(None)
        .max_ihave_length(5000)
        .iwant_followup_time(Duration::from_secs(3))
        .history_gossip(3)
        .validate_messages()
        .validation_mode(ValidationMode::Strict)
        .duplicate_cache_time(Duration::from_secs(60))
        .message_id_fn(gossip_message_id)
        .fast_message_id_fn(fast_gossip_message_id)
        .allow_self_origin(false)
        .published_message_ids_cache_time(Duration::from_secs(10))
        .build()
        .expect("valid gossipsub configuration");

    let mut gossipsub = Gossipsub::new(
        MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    )
    .expect("gossipsub initialized");

    gossipsub
        .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
        .expect("gossipsub initialized with peer score");

    gossipsub
}
