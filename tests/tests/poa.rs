use fuel_core::{
    combined_database::CombinedDatabase,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
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

#[tokio::test]
async fn can_get_sealed_block_from_poa_produced_block() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let poa_public = poa_secret.public_key();

    let db = CombinedDatabase::default();
    let mut config = Config::local_node();
    config.consensus_key = Some(Secret::new(poa_secret.into()));
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

    // check sealed block header is correct
    let sealed_block_header = db
        .on_chain()
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
    let sealed_block = db
        .on_chain()
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

#[cfg(feature = "p2p")]
mod p2p {
    use super::*;
    use fuel_core::{
        chain_config::ConsensusConfig,
        p2p_test_helpers::{
            make_config,
            make_node,
            Bootstrap,
        },
        service::ServiceTrait,
    };
    use fuel_core_poa::Trigger;
    use fuel_core_types::fuel_tx::Input;
    use std::time::Duration;

    // Starts first_producer which creates some blocks
    // Then starts second_producer that uses the first one as a reserved peer.
    // second_producer should not produce blocks while the first one is producing
    // after the first_producer stops, second_producer should start producing blocks
    #[tokio::test(flavor = "multi_thread")]
    async fn test_poa_multiple_producers() {
        const INTERVAL: u64 = 1;
        const TIMEOUT: u64 = 5 * INTERVAL;

        let mut rng = StdRng::seed_from_u64(2222);

        // Create a producer and a validator that share the same key pair.
        let secret = SecretKey::random(&mut rng);
        let pub_key = Input::owner(&secret.public_key());

        let mut config = Config::local_node();
        config.chain_conf.consensus = ConsensusConfig::PoA {
            signing_key: pub_key,
        };

        let bootstrap_config = make_config("Bootstrap".to_string(), config.clone());
        let bootstrap = Bootstrap::new(&bootstrap_config).await;

        let make_node_config = |name: &str| {
            let mut config = make_config(name.to_string(), config.clone());
            config.block_production = Trigger::Interval {
                block_time: Duration::from_secs(INTERVAL),
            };
            config.consensus_key = Some(Secret::new(secret.into()));
            config.p2p.as_mut().unwrap().bootstrap_nodes = bootstrap.listeners();
            config.p2p.as_mut().unwrap().reserved_nodes = bootstrap.listeners();
            config.min_connected_reserved_peers = 1;
            config.time_until_synced = Duration::from_secs(2 * INTERVAL);
            config
        };

        let first_producer_config = make_node_config("First Producer");
        let second_producer_config = make_node_config("Second Producer");

        let first_producer = make_node(first_producer_config, vec![]).await;

        // The first producer should produce 2 blocks.
        tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            first_producer.wait_for_blocks(2, true /* is_local */),
        )
        .await
        .expect("The first should produce 2 blocks");

        // Start the second producer after 2 blocks.
        // The second producer should synchronize 3 blocks produced by the first producer.
        let second_producer = make_node(second_producer_config, vec![]).await;
        tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            second_producer.wait_for_blocks(3, false /* is_local */),
        )
        .await
        .expect("The second should sync with the first");

        // Stop the first producer.
        // The second should start produce new blocks after 2 * `INTERVAL`
        tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            first_producer.node.stop_and_await(),
        )
        .await
        .expect("Should stop services before timeout")
        .expect("Should stop without any error");
        tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            second_producer.wait_for_blocks(1, true /* is_local */),
        )
        .await
        .expect("The second should produce one block after the first stopped");

        // Restart fresh first producer.
        // it should sync remotely 5 blocks.
        let first_producer =
            make_node(make_node_config("First Producer reborn"), vec![]).await;
        tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            first_producer.wait_for_blocks(5, false /* is_local */),
        )
        .await
        .expect("The first should reborn and sync with the second");
    }
}
