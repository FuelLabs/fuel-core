use core::time::Duration;
use fuel_core::{
    combined_database::CombinedDatabase,
    fuel_core_graphql_api::worker_service::DaCompressionConfig,
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
use fuel_core_compression::VersionedCompressedBlock;
use fuel_core_poa::signer::SignMode;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        GasCosts,
        Input,
        TransactionBuilder,
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
    let compression_config = fuel_core_compression::Config {
        temporal_registry_retention: Duration::from_secs(3600),
    };
    config.da_compression = DaCompressionConfig::Enabled(compression_config);
    let srv = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let tx =
        TransactionBuilder::script([op::ret(RegId::ONE)].into_iter().collect(), vec![])
            .max_fee_limit(0)
            .script_gas_limit(1_000_000)
            .with_gas_costs(GasCosts::free())
            .add_random_fee_input()
            .finalize_as_transaction();

    let status = client.submit_and_await_commit(&tx).await.unwrap();

    let block_height = match status {
        TransactionStatus::Success { block_height, .. } => block_height,
        other => {
            panic!("unexpected result {other:?}")
        }
    };

    let block = client.da_compressed_block(block_height).await.unwrap();
    let block = block.expect("Unable to get compressed block");
    let _: VersionedCompressedBlock = postcard::from_bytes(&block).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn da_compressed_blocks_are_available_from_non_block_producing_nodes() {
    let mut rng = StdRng::seed_from_u64(line!() as u64);

    // Create a producer and a validator that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());

    let mut config = Config::local_node();
    config.da_compression = DaCompressionConfig::Enabled(fuel_core_compression::Config {
        temporal_registry_retention: Duration::from_secs(3600),
    });

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
        Some(config),
    )
    .await;

    let producer = producers.pop().unwrap();
    let mut validator = validators.pop().unwrap();

    let v_client = FuelClient::from(validator.node.shared.graph_ql.bound_address);

    // Insert some txs
    let expected = producer.insert_txs().await;
    validator.consistency_20s(&expected).await;

    let block_height = 1u32.into();

    let block = v_client
        .da_compressed_block(block_height)
        .await
        .unwrap()
        .expect("Compressed block not available from validator");
    let _: VersionedCompressedBlock = postcard::from_bytes(&block).unwrap();
}
