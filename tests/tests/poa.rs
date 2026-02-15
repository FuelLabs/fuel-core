#![allow(unexpected_cfgs)] // for cfg(coverage)

use fuel_core::{
    combined_database::CombinedDatabase,
    service::{
        Config,
        FuelService,
        adapters::consensus_module::poa::block_path,
    },
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::consensus::Consensus,
    fuel_crypto::SecretKey,
    fuel_tx::Transaction,
    secrecy::Secret,
    signer::SignMode,
};
use rand::{
    SeedableRng,
    rngs::StdRng,
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

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_predefined_nodes_produces_these_predefined_blocks()
-> anyhow::Result<()> {
    const BLOCK_TO_PRODUCE: usize = 10;
    let mut rng = StdRng::seed_from_u64(1234);

    let directory_with_predefined_blocks = tempdir()?;
    std::fs::create_dir_all(directory_with_predefined_blocks.path())?;
    let core =
        FuelCoreDriver::spawn_feeless(&["--debug", "--poa-instant", "true"]).await?;

    for _ in 0..BLOCK_TO_PRODUCE {
        produce_block_with_tx(&mut rng, &core.client).await;
    }

    // Given
    let predefined_blocks: Vec<_> = (1..=BLOCK_TO_PRODUCE)
        .map(|block_height| {
            let block_height = block_height as u32;
            let on_chain_view =
                core.node.shared.database.on_chain().latest_view().unwrap();
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
            let on_chain_view = new_core
                .node
                .shared
                .database
                .on_chain()
                .latest_view()
                .unwrap();
            on_chain_view
                .get_full_block(&block_height.into())
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_eq!(predefined_blocks, blocks_from_new_node);
    new_core.kill().await;
    Ok(())
}

#[cfg(not(coverage))] // too slow for coverage
mod p2p {
    use super::*;
    use fuel_core::{
        chain_config::ConsensusConfig,
        p2p::Multiaddr,
        p2p_test_helpers::{
            Bootstrap,
            CustomizeConfig,
            Node,
            make_config,
            make_node,
        },
        service::config::RedisLeaderLockConfig,
    };
    use fuel_core_poa::{
        Trigger,
        ports::BlockImporter,
        service::Mode,
    };
    use fuel_core_types::{
        fuel_tx::Input,
        fuel_types::Address,
    };
    use futures::{
        StreamExt,
        stream::FuturesUnordered,
    };
    use std::{
        net::{
            SocketAddrV4,
            TcpListener,
            TcpStream,
        },
        process::{
            Child,
            Command,
            Stdio,
        },
        thread,
        time::{
            Duration,
            Instant,
            SystemTime,
            UNIX_EPOCH,
        },
    };

    // Starts first_producer which creates some blocks
    // Then starts second_producer that uses the first one as a reserved peer.
    // second_producer should not produce blocks while the first one is producing
    // after the first_producer stops, second_producer should start producing blocks
    #[tokio::test(flavor = "multi_thread")]
    async fn test_poa_multiple_producers() {
        const SYNC_TIMEOUT: u64 = 5;
        const TIME_UNTIL_SYNCED: u64 = SYNC_TIMEOUT + 10;

        let mut rng = StdRng::seed_from_u64(2222);

        // Create a producer and a validator that share the same key pair.
        let secret = SecretKey::random(&mut rng);
        let pub_key = Input::owner(&secret.public_key());

        let mut config = Config::local_node();
        update_signing_key(&mut config, pub_key);

        let bootstrap_config = make_config(
            "Bootstrap".to_string(),
            config.clone(),
            CustomizeConfig::no_overrides(),
        );
        let bootstrap = Bootstrap::new(&bootstrap_config).await.unwrap();

        let make_node_config = |name: &str| {
            let mut config = make_config(
                name.to_string(),
                config.clone(),
                CustomizeConfig::no_overrides(),
            );
            config.debug = true;
            config.block_production = Trigger::Interval {
                block_time: Duration::from_secs(1),
            };
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

        // The first producer should produce 1 block manually after `SYNC_TIMEOUT` seconds.
        first_producer
            .node
            .shared
            .poa_adapter
            .manually_produce_blocks(
                None,
                Mode::Blocks {
                    number_of_blocks: 1,
                },
            )
            .await
            .expect("The first should produce 1 block manually");

        // After 1 manual block start the second producer.
        // The first producer should produce 2 more blocks.
        // The second producer should synchronize 3(1 manual and 2 produced) blocks.
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
                    number_of_blocks: 1,
                },
            )
            .await
            .expect("The second should produce 1 blocks");
        assert!(start_time.elapsed() >= Duration::from_secs(TIME_UNTIL_SYNCED));

        // Restart fresh first producer.
        // it should sync remotely 5 blocks.
        let first_producer =
            make_node(make_node_config("First Producer reborn"), vec![]).await;
        tokio::time::timeout(
            Duration::from_secs(TIME_UNTIL_SYNCED),
            first_producer.wait_for_blocks(5, false /* is_local */),
        )
        .await
        .expect("The first should reborn and sync with the second");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_lock__four_producers__only_first_leader_produces_blocks() {
        const BLOCK_TIME: Duration = Duration::from_millis(200);
        const LEADER_ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
        const BLOCKS_TO_CHECK: usize = 30;
        const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_millis(250);

        // given
        let (_redis, _bootstrap, make_node_config) = make_leader_lock_test_config_builder(
            3333,
            BLOCK_TIME,
            "poa:failover:integration",
        )
        .await;

        let first_producer = make_node(make_node_config("First Producer"), vec![]).await;
        let second_producer =
            make_node(make_node_config("Second Producer"), vec![]).await;
        let third_producer = make_node(make_node_config("Third Producer"), vec![]).await;
        let fourth_producer =
            make_node(make_node_config("Fourth Producer"), vec![]).await;
        let all_nodes = vec![
            first_producer,
            second_producer,
            third_producer,
            fourth_producer,
        ];

        // when
        let (leader, non_leaders) =
            find_leader_and_followers(all_nodes, LEADER_ELECTION_TIMEOUT).await;

        // then
        only_first_leader_produces_blocks(
            &leader,
            &non_leaders,
            BLOCKS_TO_CHECK,
            BLOCK_IMPORT_TIMEOUT,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_lock__three_producers__leadership_handoffs_are_exclusive() {
        const BLOCK_TIME: Duration = Duration::from_millis(200);
        const LEADER_ELECTION_TIMEOUT: Duration = Duration::from_secs(2);
        const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_millis(300);
        const PHASE_BLOCKS: usize = 5;
        const STOP_TIMEOUT: Duration = Duration::from_secs(1);

        // given
        let (_redis, _bootstrap, make_node_config) =
            make_leader_lock_test_config_builder(5555, BLOCK_TIME, "poa:leader:handoff")
                .await;

        let first_producer = make_node(make_node_config("First Producer"), vec![]).await;
        let second_producer =
            make_node(make_node_config("Second Producer"), vec![]).await;
        let third_producer = make_node(make_node_config("Third Producer"), vec![]).await;

        let mut active_producers = vec![first_producer, second_producer, third_producer];

        // when
        // let all producers become leader, including the final single producer
        let mut iteration = 0usize;
        while !active_producers.is_empty() {
            eprintln!(
                "ts_ms={}, leader_handoff_iteration={iteration}, node_count={}",
                unix_time_ms(),
                active_producers.len()
            );
            let (leader, followers) =
                find_leader_and_followers(active_producers, LEADER_ELECTION_TIMEOUT)
                    .await;

            // then
            only_first_leader_produces_blocks(
                &leader,
                &followers,
                PHASE_BLOCKS,
                BLOCK_IMPORT_TIMEOUT,
            )
            .await;

            if followers.is_empty() {
                break;
            }

            eprintln!(
                "ts_ms={}, leader_handoff_iteration={iteration}, action=shutdown_leader_start",
                unix_time_ms()
            );
            tokio::time::timeout(
                STOP_TIMEOUT,
                leader.node.send_stop_signal_and_await_shutdown(),
            )
            .await
            .expect("Should stop leader before timeout")
            .expect("Should stop leader without any error");
            eprintln!(
                "ts_ms={}, leader_handoff_iteration={iteration}, action=shutdown_leader_end",
                unix_time_ms()
            );

            active_producers = followers;
            iteration += 1;
        }
    }

    async fn find_leader_and_followers(
        nodes: Vec<Node>,
        timeout: Duration,
    ) -> (Node, Vec<Node>) {
        let mut waiters = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| async move {
                wait_for_local_block(node, Some(index)).await;
                index
            })
            .collect::<FuturesUnordered<_>>();

        let node_count = nodes.len();
        let leader_index = tokio::time::timeout(timeout, async { waiters.next().await })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "No producer emitted a local block within {timeout:?} (nodes: {node_count})"
                )
            })
            .expect("Block stream ended unexpectedly before leader election");
        drop(waiters);

        let mut leader = None;
        let mut follower_nodes = Vec::with_capacity(nodes.len().saturating_sub(1));
        for (index, node) in nodes.into_iter().enumerate() {
            if index == leader_index {
                leader = Some(node);
            } else {
                follower_nodes.push(node);
            }
        }

        (leader.expect("Leader index should exist"), follower_nodes)
    }

    async fn make_leader_lock_node_config_builder(
        secret: SecretKey,
        pub_key: Address,
        block_time: Duration,
        leader_lock_config: RedisLeaderLockConfig,
        bootstrap_listeners: Vec<Multiaddr>,
    ) -> impl Fn(&str) -> Config {
        let mut base_config = Config::local_node();
        update_signing_key(&mut base_config, pub_key);

        move |name: &str| {
            let mut node_config = make_config(
                name.to_string(),
                base_config.clone(),
                CustomizeConfig::no_overrides(),
            );
            node_config.debug = true;
            node_config.block_production = Trigger::Interval { block_time };
            node_config.leader_lock = Some(leader_lock_config.clone());
            node_config.consensus_signer = SignMode::Key(Secret::new(secret.into()));
            node_config.p2p.as_mut().unwrap().bootstrap_nodes =
                bootstrap_listeners.clone();
            node_config.p2p.as_mut().unwrap().reserved_nodes =
                bootstrap_listeners.clone();
            node_config.p2p.as_mut().unwrap().info_interval =
                Some(Duration::from_millis(100));
            node_config.min_connected_reserved_peers = 1;
            node_config.time_until_synced = block_time;
            node_config
        }
    }

    async fn make_leader_lock_test_config_builder(
        seed: u64,
        block_time: Duration,
        lease_key: &str,
    ) -> (RedisTestServer, Bootstrap, impl Fn(&str) -> Config) {
        let mut rng = StdRng::seed_from_u64(seed);
        let secret = SecretKey::random(&mut rng);
        let pub_key = Input::owner(&secret.public_key());
        let mut base_config = Config::local_node();
        update_signing_key(&mut base_config, pub_key);

        let bootstrap_config = make_config(
            "Bootstrap".to_string(),
            base_config.clone(),
            CustomizeConfig::no_overrides(),
        );
        let bootstrap = Bootstrap::new(&bootstrap_config).await.unwrap();
        let bootstrap_listeners = bootstrap.listeners();

        let redis = RedisTestServer::spawn();
        let leader_lock_config = RedisLeaderLockConfig {
            redis_url: redis.redis_url(),
            lease_key: lease_key.to_string(),
            lease_ttl: Duration::from_secs(2),
        };
        let make_node_config = make_leader_lock_node_config_builder(
            secret,
            pub_key,
            block_time,
            leader_lock_config,
            bootstrap_listeners,
        )
        .await;

        (redis, bootstrap, make_node_config)
    }

    async fn wait_for_local_block(node: &Node, waiter_index: Option<usize>) {
        let mut stream = node.node.shared.block_importer.block_stream();
        let mut saw_first_block = false;
        while let Some(block) = stream.next().await {
            let height = *block.block_header.height();
            let is_local = block.is_locally_produced();
            if !saw_first_block {
                eprintln!(
                    "ts_ms={}, wait_for_local_block(waiter={:?}), first_block_seen=true",
                    unix_time_ms(),
                    waiter_index
                );
                saw_first_block = true;
            }
            eprintln!(
                "ts_ms={}, wait_for_local_block(waiter={:?}): height={height}, is_local={is_local}",
                unix_time_ms(),
                waiter_index
            );
            if is_local {
                return;
            }
        }
        panic!("block stream ended unexpectedly");
    }

    async fn wait_for_non_local_block_and_fail_on_local(node: &Node) {
        let mut stream = node.node.shared.block_importer.block_stream();
        while let Some(block) = stream.next().await {
            if block.is_locally_produced() {
                let height = *block.block_header.height();
                panic!(
                    "Expected only non-local blocks while leader is alive; got local block at height {height}"
                );
            } else {
                return;
            }
        }
        panic!("block stream ended unexpectedly");
    }

    async fn only_first_leader_produces_blocks(
        leader: &Node,
        non_leaders: &[Node],
        non_local_blocks_to_check: usize,
        block_import_timeout: Duration,
    ) {
        for _ in 0..non_local_blocks_to_check {
            tokio::time::timeout(
                block_import_timeout,
                wait_for_local_block(leader, None),
            )
                .await
                .expect("Leader should import a local block");
            for node in non_leaders {
                tokio::time::timeout(
                    block_import_timeout,
                    wait_for_non_local_block_and_fail_on_local(node),
                )
                .await
                .expect("Non-leader should import a non-local block");
            }
        }
    }

    struct RedisTestServer {
        child: Child,
        redis_url: String,
    }

    impl RedisTestServer {
        fn spawn() -> Self {
            let socket =
                TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
                    .expect("Should bind an ephemeral port");
            let port = socket.local_addr().expect("Should get local addr").port();
            drop(socket);

            let child = Command::new("redis-server")
                .arg("--port")
                .arg(port.to_string())
                .arg("--save")
                .arg("")
                .arg("--appendonly")
                .arg("no")
                .arg("--bind")
                .arg("127.0.0.1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("redis-server must be installed for this test");
            wait_for_redis_ready(port);
            Self {
                child,
                redis_url: format!("redis://127.0.0.1:{port}/"),
            }
        }

        fn redis_url(&self) -> String {
            self.redis_url.clone()
        }
    }

    impl Drop for RedisTestServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn wait_for_redis_ready(port: u16) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("redis-server did not become ready in time");
    }

    fn unix_time_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time should be after UNIX_EPOCH")
            .as_millis()
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
