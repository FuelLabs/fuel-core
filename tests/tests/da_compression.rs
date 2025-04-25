use core::time::Duration;
use fuel_core::{
    chain_config::TESTNET_WALLET_SECRETS,
    combined_database::CombinedDatabase,
    p2p_test_helpers::*,
    service::{
        config::{
            DaCompressionConfig,
            DaCompressionMode,
        },
        Config,
        FuelService,
    },
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::DatabaseConfig,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_compression::{
    decompress::decompress,
    VersionedCompressedBlock,
};
use fuel_core_compression_service::temporal_registry::{
    CompressionStorageWrapper,
    DecompressionContext,
};
use fuel_core_storage::transactional::{
    AtomicView,
    HistoricalView,
    IntoTransaction,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
    secrecy::Secret,
    signer::SignMode,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    num::NonZeroU32,
    str::FromStr,
};
use test_helpers::{
    assemble_tx::{
        AssembleAndRunTx,
        SigningAccount,
    },
    config_with_fee,
};

#[tokio::test]
async fn can_fetch_da_compressed_block_from_graphql() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);

    let mut config = config_with_fee();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    let compression_config = DaCompressionConfig {
        retention_duration: Duration::from_secs(3600),
        override_starting_height: None,
        metrics: false,
    };
    config.da_compression = DaCompressionMode::Enabled(compression_config.clone());
    let chain_id = config
        .snapshot_reader
        .chain_config()
        .consensus_parameters
        .chain_id();
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let wallet_secret =
        SecretKey::from_str(TESTNET_WALLET_SECRETS[1]).expect("Expected valid secret");

    let status = client
        .run_script(
            vec![op::ret(RegId::ONE)],
            vec![],
            SigningAccount::Wallet(wallet_secret),
        )
        .await
        .unwrap();

    let block_height = match status {
        TransactionStatus::Success { block_height, .. } => block_height,
        other => {
            panic!("unexpected result {other:?}")
        }
    };
    srv.await_compression_synced_until(&block_height)
        .await
        .unwrap();

    let block = client
        .da_compressed_block(block_height)
        .await
        .unwrap()
        .expect("Unable to get compressed block");
    let block: VersionedCompressedBlock = postcard::from_bytes(&block).unwrap();

    // Reuse the existing offchain db to decompress the block
    let db = &srv.shared.database;

    let on_chain_before_execution = db.on_chain().view_at(&0u32.into()).unwrap();
    let mut tx_inner = db.compression().clone().into_transaction();
    let db_tx = DecompressionContext {
        compression_storage: CompressionStorageWrapper {
            storage_tx: &mut tx_inner,
        },
        onchain_db: on_chain_before_execution,
    };
    let decompressed = decompress(
        fuel_core_compression::Config {
            temporal_registry_retention: compression_config.retention_duration,
        },
        db_tx,
        block,
    )
    .await
    .unwrap();

    let block_from_on_chain_db = db
        .on_chain()
        .latest_view()
        .unwrap()
        .get_full_block(&block_height)
        .unwrap()
        .unwrap();

    let db_transactions = block_from_on_chain_db.transactions();
    let decompressed_transactions = decompressed.transactions;

    assert_eq!(decompressed_transactions.len(), 2);
    for (db_tx, decompressed_tx) in
        db_transactions.iter().zip(decompressed_transactions.iter())
    {
        // ensure tx ids match
        assert_eq!(db_tx.id(&chain_id), decompressed_tx.id(&chain_id));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn da_compressed_blocks_are_available_from_non_block_producing_nodes() {
    let mut rng = StdRng::seed_from_u64(line!() as u64);

    // Create a producer and a validator that share the same key pair.
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());

    let mut config = Config::local_node();
    config.da_compression = DaCompressionMode::Enabled(DaCompressionConfig {
        retention_duration: Duration::from_secs(3600),
        override_starting_height: None,
        metrics: false,
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
    validator
        .node
        .await_compression_synced_until(&1u32.into())
        .await
        .unwrap();

    let block_height = 1u32.into();

    let compressed_block = v_client
        .da_compressed_block(block_height)
        .await
        .unwrap()
        .expect("Compressed block not available from validator");
    let _: VersionedCompressedBlock = postcard::from_bytes(&compressed_block).unwrap();
}

#[tokio::test]
async fn da_compression__starts_and_compresses_blocks_correctly_from_empty_database() {
    // given: the node starts without compression enabled, and produces blocks
    let db = CombinedDatabase::temp_database_with_state_rewind_policy(
        StateRewindPolicy::NoRewind,
        DatabaseConfig::config_for_tests(),
    )
    .unwrap();
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let blocks_to_produce = 10;

    let mut config = config_with_fee();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    config.da_compression = DaCompressionMode::Disabled;
    config.combined_db_config.state_rewind_policy = StateRewindPolicy::NoRewind;
    let srv = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let current_height = client
        .produce_blocks(blocks_to_produce, None)
        .await
        .unwrap();
    assert_eq!(current_height, blocks_to_produce.into());

    let mut config = srv.shared.config.clone();

    // when: the node is restarted with compression enabled, and blocks are produced
    config.da_compression = DaCompressionMode::Enabled(DaCompressionConfig {
        retention_duration: Duration::from_secs(3600),
        override_starting_height: None,
        metrics: false,
    });
    let srv = FuelService::from_combined_database(db, config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let current_height = client
        .produce_blocks(blocks_to_produce, None)
        .await
        .unwrap();
    assert_eq!(current_height, BlockHeight::from(blocks_to_produce * 2));

    srv.await_compression_synced_until(&current_height)
        .await
        .unwrap();

    // then: the da compressed blocks from height 1 to height blocks_to_produce don't exist
    // and the da compressed blocks from height blocks_to_produce + 1 to height blocks_to_produce * 2 exist
    for height in 0..=blocks_to_produce {
        let compressed_block = client.da_compressed_block(height.into()).await.unwrap();
        assert!(compressed_block.is_none());
    }

    for height in blocks_to_produce + 1..=blocks_to_produce * 2 {
        let compressed_block = client.da_compressed_block(height.into()).await.unwrap();
        assert!(compressed_block.is_some());
    }
}

#[tokio::test]
async fn da_compression__starts_and_compresses_blocks_correctly_with_overriden_height() {
    // given: the node starts without compression enabled, and produces blocks
    let db = CombinedDatabase::temp_database_with_state_rewind_policy(
        StateRewindPolicy::RewindFullRange,
        DatabaseConfig::config_for_tests(),
    )
    .unwrap();
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let blocks_to_produce = 10;

    let mut config = config_with_fee();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    config.da_compression = DaCompressionMode::Disabled;
    config.combined_db_config.state_rewind_policy = StateRewindPolicy::NoRewind;
    let srv = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let current_height = client
        .produce_blocks(blocks_to_produce, None)
        .await
        .unwrap();
    assert_eq!(current_height, blocks_to_produce.into());

    let mut config = srv.shared.config.clone();

    // when: the node is restarted with compression enabled, starting height overriden, blocks are produced
    let override_starting_height = 10;
    config.da_compression = DaCompressionMode::Enabled(DaCompressionConfig {
        retention_duration: Duration::from_secs(3600),
        override_starting_height: Some(
            NonZeroU32::new(override_starting_height).unwrap(),
        ),
        metrics: false,
    });
    let srv = FuelService::from_combined_database(db, config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let current_height = client
        .produce_blocks(blocks_to_produce, None)
        .await
        .unwrap();
    assert_eq!(current_height, BlockHeight::from(blocks_to_produce * 2));

    srv.await_compression_synced_until(&current_height)
        .await
        .unwrap();

    // then: the da compressed blocks from height 1 to height override_starting_height
    // and the da compressed blocks from height override_starting_height to height blocks_to_produce * 2 exist
    for height in 0..override_starting_height {
        let compressed_block = client.da_compressed_block(height.into()).await.unwrap();
        assert!(compressed_block.is_none());
    }

    for height in override_starting_height..=blocks_to_produce * 2 {
        let compressed_block = client.da_compressed_block(height.into()).await.unwrap();
        assert!(compressed_block.is_some());
    }
}
