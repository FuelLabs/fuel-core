use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;

#[tokio::test]
async fn chain_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let chain_info = client.chain_info().await.unwrap();

    assert_eq!(0, chain_info.da_height);
    assert_eq!(node_config.chain_conf.chain_name, chain_info.name);
    assert_eq!(
        node_config.chain_conf.consensus_parameters,
        chain_info.consensus_parameters.clone()
    );

    assert_eq!(
        node_config.chain_conf.consensus_parameters.gas_costs,
        chain_info.consensus_parameters.gas_costs
    );
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

    let validator_peer_id = validator.node.shared.config.p2p.unwrap().keypair.public().to_peer_id();

    let peers = producer.node.shared.network.unwrap().get_all_peers().await.unwrap();

    assert_eq!(peers.len(), 2);
    let (_, validator_info) = peers.iter().find(|(peer_id, _)| {
        peer_id.clone() == validator_peer_id
    }).expect("Should be connected to validator");
    
    assert!(validator_info.heartbeat_data.last_heartbeat.elapsed() < Duration::from_secs(10));

}
