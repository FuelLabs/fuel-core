use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    types::NodeInfo,
    FuelClient,
};

#[tokio::test]
async fn node_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let NodeInfo {
        utxo_validation,
        vm_backtrace,
        max_depth,
        max_tx,
        ..
    } = client.node_info().await.unwrap();

    assert_eq!(utxo_validation, node_config.utxo_validation);
    assert_eq!(vm_backtrace, node_config.vm.backtrace);
    assert_eq!(max_depth, node_config.txpool.max_depth as u64);
    assert_eq!(max_tx, node_config.txpool.max_tx as u64);
}

#[cfg(feature = "p2p")]
#[tokio::test(flavor = "multi_thread")]
async fn test_peer_info() {
    use fuel_core::p2p_test_helpers::{
        make_nodes,
        BootstrapSetup,
        Nodes,
        ProducerSetup,
        ValidatorSetup,
    };
    use fuel_core_types::{
        fuel_tx::Input,
        fuel_vm::SecretKey,
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };
    use std::time::Duration;

    let mut rng = StdRng::seed_from_u64(line!() as u64);

    // Create a producer and a validator that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        [Some(BootstrapSetup::new(pub_key))],
        [Some(
            ProducerSetup::new(secret).with_txs(1).with_name("Alice"),
        )],
        [Some(ValidatorSetup::new(pub_key).with_name("Bob"))],
        None,
    )
    .await;

    let producer = producers.pop().unwrap();
    let mut validator = validators.pop().unwrap();

    // Insert the transactions into the tx pool and await them,
    // to ensure we have a live p2p connection.
    let expected = producer.insert_txs().await;

    // Wait up to 10 seconds for the validator to sync with the producer.
    // This indicates we have a successful P2P connection.
    validator.consistency_10s(&expected).await;

    let validator_peer_id = validator
        .node
        .shared
        .config
        .p2p
        .as_ref()
        .unwrap()
        .keypair
        .public()
        .to_peer_id();

    // TODO: this needs to fetch peers from the GQL API, not the service directly.
    // This is just a mock of what we should be able to do with GQL API.
    let client = producer.node.bound_address;
    let client = FuelClient::from(client);
    let peers = client.connected_peers_info().await.unwrap();
    assert_eq!(peers.len(), 2);
    let info = peers
        .iter()
        .find(|info| info.id.to_string() == validator_peer_id.to_base58())
        .expect("Should be connected to validator");

    let time_since_heartbeat = std::time::SystemTime::now()
        .duration_since(info.heartbeat_data.last_heartbeat)
        .unwrap();
    assert!(time_since_heartbeat < Duration::from_secs(10));
}
