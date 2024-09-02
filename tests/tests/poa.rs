#![allow(unexpected_cfgs)] // for cfg(coverage)

use fuel_core::{
    combined_database::CombinedDatabase,
    service::{
        adapters::consensus_module::poa::block_path,
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::signer::SignMode;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::consensus::Consensus,
    fuel_crypto::SecretKey,
    fuel_tx::Transaction,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use tempfile::tempdir;
use test_helpers::{
    fuel_core_driver::FuelCoreDriver,
    produce_block_with_tx,
};

#[tokio::test]
async fn can_get_sealed_block_from_poa_produced_block() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let poa_public = poa_secret.public_key();

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

    let view = db.on_chain().latest_view().unwrap();

    // check sealed block header is correct
    let sealed_block_header = view
        .get_sealed_block_header(&block_height)
        .unwrap()
        .expect("expected sealed header to be available");

    // verify signature
    let block_id = sealed_block_header.entity.id();
    let block_height = sealed_block_header.entity.height();
    let signature = match sealed_block_header.consensus {
        Consensus::PoA(poa) => poa.signature,
        _ => panic!("Not expected consensus"),
    };
    signature
        .verify(&poa_public, &block_id.into_message())
        .expect("failed to verify signature");

    // check sealed block is correct
    let sealed_block = view
        .get_sealed_block_by_height(block_height)
        .unwrap()
        .expect("expected sealed header to be available");

    // verify signature
    let block_id = sealed_block.entity.id();
    let signature = match sealed_block.consensus {
        Consensus::PoA(poa) => poa.signature,
        _ => panic!("Not expected consensus"),
    };
    signature
        .verify(&poa_public, &block_id.into_message())
        .expect("failed to verify signature");
}

#[tokio::test]
#[cfg(feature = "aws-kms")]
async fn can_get_sealed_block_from_poa_produced_block_when_signing_with_kms() {
    use fuel_core_types::fuel_crypto::PublicKey;
    use k256::pkcs8::DecodePublicKey;

    // This test is only enabled if the environment variable is set
    let Some(kms_arn) = option_env!("FUEL_CORE_TEST_AWS_KMS_ARN") else {
        return;
    };

    // Get the public key for the KMS key
    let config = aws_config::load_from_env().await;
    let kms_client = aws_sdk_kms::Client::new(&config);
    let poa_public_der = kms_client
        .get_public_key()
        .key_id(kms_arn)
        .send()
        .await
        .expect("Unable to fetch public key from KMS")
        .public_key
        .unwrap()
        .into_inner();
    let poa_public = k256::PublicKey::from_public_key_der(&poa_public_der)
        .expect("invalid DER public key from AWS KMS");
    let poa_public = PublicKey::from(poa_public);

    // start node with the kms enabled and produce some blocks
    let num_blocks = 100;
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-aws-kms",
        kms_arn,
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    let _ = driver
        .client
        .produce_blocks(num_blocks, None)
        .await
        .unwrap();

    // stop the node and just grab the database
    let db_path = driver.kill().await;
    let db =
        CombinedDatabase::open(db_path.path(), 1024 * 1024, Default::default()).unwrap();

    let view = db.on_chain().latest_view().unwrap();

    // verify signatures and ensure that the block producer wont change
    let mut block_producer = None;
    for height in 1..=num_blocks {
        let sealed_block = view
            .get_sealed_block_by_height(&height.into())
            .unwrap()
            .expect("expected sealed block to be available");
        let block_id = sealed_block.entity.id();
        let signature = match sealed_block.consensus {
            Consensus::PoA(ref poa) => poa.signature,
            _ => panic!("Not expected consensus"),
        };
        signature
            .verify(&poa_public, &block_id.into_message())
            .expect("failed to verify signature");
        let this_bp = sealed_block
            .consensus
            .block_producer(&block_id)
            .expect("Block should have a block producer");
        if let Some(bp) = block_producer {
            assert_eq!(bp, this_bp, "Block producer changed");
        } else {
            block_producer = Some(this_bp);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_predefined_nodes_produces_these_predefined_blocks(
) -> anyhow::Result<()> {
    const BLOCK_TO_PRODUCE: usize = 10;
    let mut rng = StdRng::seed_from_u64(1234);

    let directory_with_predefined_blocks = tempdir()?;
    std::fs::create_dir_all(directory_with_predefined_blocks.path())?;
    let core =
        FuelCoreDriver::spawn_feeless(&["--debug", "--poa-instant", "true"]).await?;

    for _ in 0..BLOCK_TO_PRODUCE {
        produce_block_with_tx(&mut rng, &core.client).await;
    }
    let on_chain_view = core.node.shared.database.on_chain().latest_view()?;

    // Given
    let predefined_blocks: Vec<_> = (1..=BLOCK_TO_PRODUCE)
        .map(|block_height| {
            let block_height = block_height as u32;
            on_chain_view
                .get_full_block(&block_height.into())
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_eq!(predefined_blocks.len(), BLOCK_TO_PRODUCE);
    core.kill().await;
    for block in &predefined_blocks {
        let json = serde_json::to_string_pretty(block)?;
        let height: u32 = (*block.header().height()).into();
        let path = block_path(directory_with_predefined_blocks.path(), height);
        std::fs::write(path, json)?;
    }

    // When
    let new_core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--predefined-blocks-path",
        directory_with_predefined_blocks.path().to_str().unwrap(),
    ])
    .await?;

    // Then
    let expected_height = BLOCK_TO_PRODUCE as u32;
    new_core
        .wait_for_block_height_10s(&expected_height.into())
        .await;
    let blocks_from_new_node: Vec<_> = (1..=BLOCK_TO_PRODUCE)
        .map(|block_height| {
            let block_height = block_height as u32;
            on_chain_view
                .get_full_block(&block_height.into())
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_eq!(predefined_blocks, blocks_from_new_node);

    Ok(())
}

#[cfg(feature = "p2p")]
#[cfg(not(coverage))] // too slow for coverage
mod p2p {
    use super::*;
    use fuel_core::{
        chain_config::ConsensusConfig,
        p2p_test_helpers::{
            make_config,
            make_node,
            Bootstrap,
        },
    };
    use fuel_core_poa::{
        service::Mode,
        Trigger,
    };
    use fuel_core_types::{
        fuel_tx::Input,
        fuel_types::Address,
    };
    use std::time::Duration;

    // Starts first_producer which creates some blocks
    // Then starts second_producer that uses the first one as a reserved peer.
    // second_producer should not produce blocks while the first one is producing
    // after the first_producer stops, second_producer should start producing blocks
    #[tokio::test(flavor = "multi_thread")]
    async fn test_poa_multiple_producers() {
        const SYNC_TIMEOUT: u64 = 30;
        const TIME_UNTIL_SYNCED: u64 = SYNC_TIMEOUT + 10;

        let mut rng = StdRng::seed_from_u64(2222);

        // Create a producer and a validator that share the same key pair.
        let secret = SecretKey::random(&mut rng);
        let pub_key = Input::owner(&secret.public_key());

        let mut config = Config::local_node();
        update_signing_key(&mut config, pub_key);

        let bootstrap_config = make_config("Bootstrap".to_string(), config.clone());
        let bootstrap = Bootstrap::new(&bootstrap_config).await;

        let make_node_config = |name: &str| {
            let mut config = make_config(name.to_string(), config.clone());
            config.debug = true;
            config.block_production = Trigger::Never;
            config.consensus_signer = SignMode::Key(Secret::new(secret.into()));
            config.p2p.as_mut().unwrap().bootstrap_nodes = bootstrap.listeners();
            config.p2p.as_mut().unwrap().reserved_nodes = bootstrap.listeners();
            config.p2p.as_mut().unwrap().info_interval = Some(Duration::from_millis(100));
            config.min_connected_reserved_peers = 1;
            config.time_until_synced = Duration::from_secs(TIME_UNTIL_SYNCED);
            config
        };

        let first_producer_config = make_node_config("First Producer");
        let second_producer_config = make_node_config("Second Producer");

        let first_producer = make_node(first_producer_config, vec![]).await;

        // The first producer should produce 3 blocks.
        first_producer
            .node
            .shared
            .poa_adapter
            .manually_produce_blocks(
                None,
                Mode::Blocks {
                    number_of_blocks: 3,
                },
            )
            .await
            .expect("The first should produce 3 blocks");

        // Start the second producer after 3 blocks.
        // The second producer should synchronize 3 blocks produced by the first producer.
        let second_producer = make_node(second_producer_config, vec![]).await;
        tokio::time::timeout(
            Duration::from_secs(SYNC_TIMEOUT),
            second_producer.wait_for_blocks(3, false /* is_local */),
        )
        .await
        .expect("The second should sync with the first");

        let start_time = tokio::time::Instant::now();
        // Stop the first producer.
        tokio::time::timeout(
            Duration::from_secs(1),
            first_producer.node.send_stop_signal_and_await_shutdown(),
        )
        .await
        .expect("Should stop services before timeout")
        .expect("Should stop without any error");

        // The second should start produce new blocks after `TIMEOUT`
        second_producer
            .node
            .shared
            .poa_adapter
            .manually_produce_blocks(
                None,
                Mode::Blocks {
                    number_of_blocks: 2,
                },
            )
            .await
            .expect("The second should produce 2 blocks");
        assert!(start_time.elapsed() >= Duration::from_secs(TIME_UNTIL_SYNCED));

        // Restart fresh first producer.
        // it should sync remotely 5 blocks.
        let first_producer =
            make_node(make_node_config("First Producer reborn"), vec![]).await;
        tokio::time::timeout(
            Duration::from_secs(SYNC_TIMEOUT),
            first_producer.wait_for_blocks(5, false /* is_local */),
        )
        .await
        .expect("The first should reborn and sync with the second");
    }

    fn update_signing_key(config: &mut Config, key: Address) {
        let snapshot_reader = &config.snapshot_reader;
        let mut chain_config = snapshot_reader.chain_config().clone();
        match &mut chain_config.consensus {
            ConsensusConfig::PoA { signing_key } => {
                *signing_key = key;
            }
            ConsensusConfig::PoAV2(poa) => {
                poa.set_genesis_signing_key(key);
            }
        }
        config.snapshot_reader = snapshot_reader.clone().with_chain_config(chain_config)
    }
}
