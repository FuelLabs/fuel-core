use fuel_core::{
    combined_database::CombinedDatabase,
    p2p_test_helpers::*,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::signer::SignMode;
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        Transaction,
    },
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::test]
async fn can_fetch_da_compressed_block_from_graphql() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);

    let db = CombinedDatabase::default();
    let mut config = Config::local_node();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    let srv = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let status = client
        .submit_and_await_commit(&Transaction::default_test_tx())
        .await
        .unwrap();

    let block_height = match status {
        TransactionStatus::Success { block_height, .. } => block_height,
        _ => {
            panic!("unexpected result")
        }
    };

    let block = client.da_compressed_block(block_height).await.unwrap();
    assert!(block.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn da_compressed_blocks_are_available_from_non_block_producing_nodes() {
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
        Some(Config {
            debug: true,
            utxo_validation: false,
            ..Config::local_node()
        }),
    )
    .await;

    let mut producer = producers.pop().unwrap();
    let mut validator = validators.pop().unwrap();

    let p_client = FuelClient::from(producer.node.shared.graph_ql.bound_address);
    let v_client = FuelClient::from(validator.node.shared.graph_ql.bound_address);

    // Insert some txs
    let expected = producer.insert_txs().await;
    producer.consistency_10s(&expected).await;
    validator.consistency_20s(&expected).await;

    let block_height = 1u32.into();

    let p_block = p_client
        .da_compressed_block(block_height)
        .await
        .unwrap()
        .expect("Compressed block not available from producer");

    let v_block = v_client
        .da_compressed_block(block_height)
        .await
        .unwrap()
        .expect("Compressed block not available from validator");

    assert!(p_block == v_block);
}
