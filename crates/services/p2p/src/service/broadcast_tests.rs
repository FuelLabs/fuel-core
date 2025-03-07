#![allow(non_snake_case)]

use super::*;
use fuel_core_types::services::p2p::PreConfirmationMessage;

fn arb_shared_state() -> SharedState {
    let config = Config::default("test network");
    let (shared_state, _) = build_shared_state(config);
    shared_state
}

#[tokio::test]
async fn shared_state__broadcast__tx_preconfirmations() {
    // given
    let broadcast = arb_shared_state();
    let preconfirmations = PreConfirmationMessage::default_test_confirmation();
    let preconfirmations_gossip_data = P2PPreConfirmationGossipData {
        data: Some(preconfirmations.clone()),
        peer_id: FuelPeerId::from(PeerId::random().to_bytes().to_vec()),
        message_id: vec![1, 2, 3, 4],
    };
    let mut preconfirmations_receiver = broadcast.subscribe_confirmations();

    // when
    broadcast
        .pre_confirmation_broadcast(preconfirmations_gossip_data)
        .unwrap();

    // then
    let actual = preconfirmations_receiver.try_recv().unwrap().data.unwrap();
    assert_eq!(preconfirmations, actual);
}
