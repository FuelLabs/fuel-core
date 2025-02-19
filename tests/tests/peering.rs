#![allow(non_snake_case)]

use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    CustomizeConfig,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::Input,
};
use rand::{
    prelude::StdRng,
    SeedableRng,
};
use std::time::Duration;

#[tokio::test]
async fn max_discovery_peers_connected__node_will_not_discover_new_nodes_if_full() {
    let mut rng = StdRng::seed_from_u64(1234);

    // given
    let expected = 3usize;

    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let producer_overrides =
        CustomizeConfig::no_overrides().max_discovery_peers_connected(expected as u32);
    let producer_setup =
        ProducerSetup::new_with_overrides(secret, producer_overrides).with_name("Alice");
    let bootstrap = BootstrapSetup::new(pub_key);
    let many_validators = (0..expected + 10).map(|_| Some(ValidatorSetup::new(pub_key)));

    // when
    let nodes = make_nodes(
        [Some(bootstrap)],
        [Some(producer_setup)],
        many_validators,
        None,
    )
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // then
    let Nodes { mut producers, .. } = nodes;
    let producer = producers.pop().unwrap();
    let client = FuelClient::from(producer.node.bound_address);

    let peering_info = client.connected_peers_info().await.unwrap();
    let actual = peering_info.len();

    assert_eq!(expected, actual);
}

#[tokio::test]
async fn max_discovery_peers_connected__nodes_will_discover_new_peers_if_first_peer_is_full(
) {
    let mut rng = StdRng::seed_from_u64(1234);

    // given
    // one for bootstrap, one for a single validator
    let producer_max_peers = 2;
    let new_blocks = 10;

    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let producer_overrides =
        CustomizeConfig::no_overrides().max_discovery_peers_connected(producer_max_peers);
    let producer_setup =
        ProducerSetup::new_with_overrides(secret, producer_overrides).with_name("Alice");
    let bootstrap = BootstrapSetup::new(pub_key);
    let many_validators = (0..10).map(|_| Some(ValidatorSetup::new(pub_key)));

    // when
    let nodes = make_nodes(
        [Some(bootstrap)],
        [Some(producer_setup)],
        many_validators,
        None,
    )
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // then
    let Nodes {
        mut producers,
        validators,
        ..
    } = nodes;
    let producer = producers.pop().unwrap();
    let client = FuelClient::from(producer.node.bound_address);
    client.produce_blocks(new_blocks, None).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;

    for validator in validators {
        let client = FuelClient::from(validator.node.bound_address);
        let latest_block_height = client
            .chain_info()
            .await
            .unwrap()
            .latest_block
            .header
            .height;
        assert_eq!(latest_block_height, new_blocks);
    }
}
