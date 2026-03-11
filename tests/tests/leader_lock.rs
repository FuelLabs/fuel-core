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
    service::{
        Config,
        config::RedisLeaderLockConfig,
    },
};
use fuel_core_poa::{
    Trigger,
    ports::BlockImporter,
};
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::Input,
    fuel_types::Address,
    secrecy::Secret,
    signer::SignMode,
};
use futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use rand::{
    SeedableRng,
    rngs::StdRng,
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
    },
};

#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__four_producers__only_first_leader_produces_blocks() {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LEADER_ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
    const BLOCKS_TO_CHECK: usize = 30;
    const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_secs(2);

    // given
    let (_redis, _bootstrap, make_node_config) = make_leader_lock_test_config_builder(
        3333,
        BLOCK_TIME,
        "poa:failover:integration",
    )
    .await;

    let first_producer = make_node(make_node_config("First Producer"), vec![]).await;
    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;
    let third_producer = make_node(make_node_config("Third Producer"), vec![]).await;
    let fourth_producer = make_node(make_node_config("Fourth Producer"), vec![]).await;
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
    only_leader_produces_blocks(
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
    const LEADER_ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
    const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_secs(2);
    const PHASE_BLOCKS: usize = 5;
    const STOP_TIMEOUT: Duration = Duration::from_secs(1);

    // given
    let (_redis, _bootstrap, make_node_config) =
        make_leader_lock_test_config_builder(5555, BLOCK_TIME, "poa:leader:handoff")
            .await;

    let first_producer = make_node(make_node_config("First Producer"), vec![]).await;
    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;
    let third_producer = make_node(make_node_config("Third Producer"), vec![]).await;

    let mut active_producers = vec![first_producer, second_producer, third_producer];

    // when
    // let all producers become leader, including the final single producer
    while !active_producers.is_empty() {
        let (leader, followers) =
            find_leader_and_followers(active_producers, LEADER_ELECTION_TIMEOUT).await;

        // then
        only_leader_produces_blocks(
            &leader,
            &followers,
            PHASE_BLOCKS,
            BLOCK_IMPORT_TIMEOUT,
        )
        .await;

        if followers.is_empty() {
            break;
        }

        tokio::time::timeout(
            STOP_TIMEOUT,
            leader.node.send_stop_signal_and_await_shutdown(),
        )
        .await
        .expect("Should stop leader before timeout")
        .expect("Should stop leader without any error");

        active_producers = followers;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__two_producers__when_first_restarts_then_second_keeps_lock() {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LEADER_ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
    const BLOCK_IMPORT_TIMEOUT: Duration = Duration::from_secs(2);
    const BLOCKS_BEFORE_FAILOVER: usize = 3;
    const BLOCKS_AFTER_RESTART: usize = 5;
    const FIRST_PRODUCER_NO_LOCAL_BLOCK_TIMEOUT: Duration = Duration::from_secs(60);
    const STOP_TIMEOUT: Duration = Duration::from_secs(1);

    // given
    let (_redis, _bootstrap, make_node_config) = make_leader_lock_test_config_builder(
        7777,
        BLOCK_TIME,
        "poa:leader:restart-lock-retention",
    )
    .await;

    let mut first_producer = make_node(make_node_config("First Producer"), vec![]).await;
    tokio::time::timeout(
        LEADER_ELECTION_TIMEOUT,
        wait_for_local_block(&first_producer),
    )
    .await
    .expect("First producer should acquire leadership initially");
    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;

    only_leader_produces_blocks(
        &first_producer,
        std::slice::from_ref(&second_producer),
        BLOCKS_BEFORE_FAILOVER,
        BLOCK_IMPORT_TIMEOUT,
    )
    .await;

    // when
    tokio::time::timeout(STOP_TIMEOUT, first_producer.shutdown())
        .await
        .expect("Should stop first producer before timeout");
    tokio::time::timeout(
        LEADER_ELECTION_TIMEOUT,
        wait_for_local_block(&second_producer),
    )
    .await
    .expect("Second producer should acquire leadership after first shutdown");

    first_producer.start().await;

    // then
    only_leader_produces_blocks(
        &second_producer,
        std::slice::from_ref(&first_producer),
        BLOCKS_AFTER_RESTART,
        BLOCK_IMPORT_TIMEOUT,
    )
    .await;

    let first_producer_local_block_result = tokio::time::timeout(
        FIRST_PRODUCER_NO_LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block(&first_producer),
    )
    .await;
    assert!(
        first_producer_local_block_result.is_err(),
        "First producer should not produce local blocks after restart while second holds the lock"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__single_producer__when_quorum_is_restored_then_production_starts() {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const NO_QUORUM_TIMEOUT: Duration = Duration::from_secs(2);
    const QUORUM_RECOVERY_TIMEOUT: Duration = Duration::from_secs(5);

    // given
    let first_redis = RedisTestServer::spawn();
    let mut second_redis = RedisTestServer::new_stopped();
    let third_redis = RedisTestServer::new_stopped();
    let (_bootstrap, make_node_config) =
        make_leader_lock_test_config_builder_with_redis_urls(
            6666,
            BLOCK_TIME,
            "poa:leader:quorum-recovery",
            vec![
                first_redis.redis_url(),
                second_redis.redis_url(),
                third_redis.redis_url(),
            ],
        )
        .await;
    let producer = make_node(make_node_config("Producer"), vec![]).await;

    // when
    let no_quorum_result =
        tokio::time::timeout(NO_QUORUM_TIMEOUT, wait_for_local_block(&producer)).await;
    second_redis.start();
    // third_redis.start();
    let quorum_result =
        tokio::time::timeout(QUORUM_RECOVERY_TIMEOUT, wait_for_local_block(&producer))
            .await;

    // then
    assert!(
        no_quorum_result.is_err(),
        "Producer should not produce while quorum is unavailable"
    );
    assert!(
        quorum_result.is_ok(),
        "Producer should produce once quorum servers are available"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__two_producers__when_second_starts_after_first_shutdown_then_second_builds_on_redis_height()
 {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LOCAL_BLOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const STOP_TIMEOUT: Duration = Duration::from_secs(1);

    // given
    let (_redis, _bootstrap, make_node_config) = make_leader_lock_test_config_builder(
        8888,
        BLOCK_TIME,
        "poa:leader:redis-reconciliation",
    )
    .await;

    let mut first_producer = make_node(make_node_config("First Producer"), vec![]).await;

    // when
    let first_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&first_producer),
    )
    .await
    .expect("First producer should produce a local block");

    tokio::time::timeout(STOP_TIMEOUT, first_producer.shutdown())
        .await
        .expect("Should stop first producer before timeout");

    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;
    let second_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&second_producer),
    )
    .await
    .expect("Second producer should produce a local block");

    // then
    assert_eq!(
        first_height, 1,
        "First producer should first produce block at height 1"
    );
    assert_eq!(
        second_height, 2,
        "Second producer should first produce block at height 2 by reconciling Redis stream"
    );
}

/// When one of three Redis nodes is down during a leadership handoff,
/// the new leader should still reconcile blocks from the remaining two
/// nodes (quorum) and continue producing at the correct height.
#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__two_producers__when_one_redis_node_is_down_then_handoff_reconciles_correctly()
 {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LOCAL_BLOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const STOP_TIMEOUT: Duration = Duration::from_secs(1);

    // given — 3 Redis nodes, all running
    let redis_a = RedisTestServer::spawn();
    let redis_b = RedisTestServer::spawn();
    let mut redis_c = RedisTestServer::spawn();
    let redis_urls = vec![
        redis_a.redis_url(),
        redis_b.redis_url(),
        redis_c.redis_url(),
    ];
    let (_bootstrap, make_node_config) =
        make_leader_lock_test_config_builder_with_redis_urls(
            9991,
            BLOCK_TIME,
            "poa:leader:degraded-handoff",
            redis_urls,
        )
        .await;

    let mut first_producer = make_node(make_node_config("First Producer"), vec![]).await;
    let first_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&first_producer),
    )
    .await
    .expect("First producer should produce a local block");

    // when — kill one Redis node, then shut down the leader
    redis_c.stop();

    tokio::time::timeout(STOP_TIMEOUT, first_producer.shutdown())
        .await
        .expect("Should stop first producer before timeout");

    // Second producer starts with only 2/3 Redis nodes up
    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;
    let second_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&second_producer),
    )
    .await
    .expect("Second producer should produce a local block with degraded Redis");

    // then — second producer should reconcile from 2 surviving nodes
    // and continue from the correct height
    assert_eq!(first_height, 1);
    assert!(
        second_height > first_height,
        "Second producer should build on first producer's height (got {second_height}, first was {first_height})"
    );
}

/// When a Redis node restarts (losing all in-memory data) during a
/// leadership handoff, the new leader should still reconcile from the
/// remaining nodes that have the data and continue at the correct height.
#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__two_producers__when_redis_node_restarts_then_handoff_reconciles_from_surviving_nodes()
 {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LOCAL_BLOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const STOP_TIMEOUT: Duration = Duration::from_secs(1);

    // given — 3 Redis nodes, all running
    let redis_a = RedisTestServer::spawn();
    let redis_b = RedisTestServer::spawn();
    let mut redis_c = RedisTestServer::spawn();
    let redis_urls = vec![
        redis_a.redis_url(),
        redis_b.redis_url(),
        redis_c.redis_url(),
    ];
    let (_bootstrap, make_node_config) =
        make_leader_lock_test_config_builder_with_redis_urls(
            9992,
            BLOCK_TIME,
            "poa:leader:data-loss-handoff",
            redis_urls,
        )
        .await;

    let mut first_producer = make_node(make_node_config("First Producer"), vec![]).await;
    let first_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&first_producer),
    )
    .await
    .expect("First producer should produce a local block");

    // when — restart one Redis node (loses all stream data), then handoff
    redis_c.stop();
    redis_c.start();

    tokio::time::timeout(STOP_TIMEOUT, first_producer.shutdown())
        .await
        .expect("Should stop first producer before timeout");

    let second_producer = make_node(make_node_config("Second Producer"), vec![]).await;
    let second_height = tokio::time::timeout(
        LOCAL_BLOCK_TIMEOUT,
        wait_for_local_block_height(&second_producer),
    )
    .await
    .expect("Second producer should produce after Redis data loss on one node");

    // then — second producer reconciles from 2 surviving nodes (a, b)
    // and continues at the correct height despite node c having no data
    assert_eq!(first_height, 1);
    assert!(
        second_height > first_height,
        "Second producer should build on first producer's height (got {second_height}, first was {first_height})"
    );
}

/// When two of three Redis nodes go down while a producer is running,
/// the producer should stop producing (cannot reach quorum for lease
/// or reconciliation) rather than producing divergent blocks.
#[tokio::test(flavor = "multi_thread")]
async fn leader_lock__single_producer__when_two_redis_nodes_go_down_then_production_stops()
 {
    const BLOCK_TIME: Duration = Duration::from_millis(200);
    const LOCAL_BLOCK_TIMEOUT: Duration = Duration::from_secs(5);
    const NO_PRODUCTION_TIMEOUT: Duration = Duration::from_secs(3);

    // given — 3 Redis nodes, all running, producer active
    let redis_a = RedisTestServer::spawn();
    let mut redis_b = RedisTestServer::spawn();
    let mut redis_c = RedisTestServer::spawn();
    let redis_urls = vec![
        redis_a.redis_url(),
        redis_b.redis_url(),
        redis_c.redis_url(),
    ];
    let (_bootstrap, make_node_config) =
        make_leader_lock_test_config_builder_with_redis_urls(
            9993,
            BLOCK_TIME,
            "poa:leader:quorum-loss",
            redis_urls,
        )
        .await;

    let producer = make_node(make_node_config("Producer"), vec![]).await;
    tokio::time::timeout(LOCAL_BLOCK_TIMEOUT, wait_for_local_block(&producer))
        .await
        .expect("Producer should produce blocks with full quorum");

    // when — kill 2 of 3 Redis nodes
    redis_b.stop();
    redis_c.stop();

    // then — producer should stop producing (can't reach quorum)
    let result =
        tokio::time::timeout(NO_PRODUCTION_TIMEOUT, wait_for_local_block(&producer))
            .await;
    assert!(
        result.is_err(),
        "Producer should not produce new blocks when Redis quorum is lost"
    );
}

async fn find_leader_and_followers(
    nodes: Vec<Node>,
    timeout: Duration,
) -> (Node, Vec<Node>) {
    let mut waiters = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| async move {
            wait_for_local_block(node).await;
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
        node_config.p2p.as_mut().unwrap().bootstrap_nodes = bootstrap_listeners.clone();
        node_config.p2p.as_mut().unwrap().reserved_nodes = bootstrap_listeners.clone();
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
    let redis = RedisTestServer::spawn();
    let (bootstrap, make_node_config) =
        make_leader_lock_test_config_builder_with_redis_urls(
            seed,
            block_time,
            lease_key,
            vec![redis.redis_url()],
        )
        .await;

    (redis, bootstrap, make_node_config)
}

async fn make_leader_lock_test_config_builder_with_redis_urls(
    seed: u64,
    block_time: Duration,
    lease_key: &str,
    redis_urls: Vec<String>,
) -> (Bootstrap, impl Fn(&str) -> Config) {
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

    let leader_lock_config = RedisLeaderLockConfig {
        redis_urls,
        lease_key: lease_key.to_string(),
        lease_ttl: Duration::from_secs(2),
        node_timeout: Duration::from_millis(100),
        retry_delay: Duration::from_millis(100),
        max_retry_delay_offset: Duration::from_millis(25),
        max_attempts: 2,
        stream_max_len: 1000,
    };
    let make_node_config = make_leader_lock_node_config_builder(
        secret,
        pub_key,
        block_time,
        leader_lock_config,
        bootstrap_listeners,
    )
    .await;

    (bootstrap, make_node_config)
}

async fn wait_for_local_block(node: &Node) {
    let mut stream = node.node.shared.block_importer.block_stream();
    let mut saw_first_block = false;
    while let Some(block) = stream.next().await {
        let is_local = block.is_locally_produced();
        if !saw_first_block {
            saw_first_block = true;
        }
        if is_local {
            return;
        }
    }
    panic!("block stream ended unexpectedly");
}

async fn wait_for_local_block_height(node: &Node) -> u32 {
    let mut stream = node.node.shared.block_importer.block_stream();
    while let Some(block) = stream.next().await {
        let height = *block.block_header.height();
        let is_local = block.is_locally_produced();
        if is_local {
            return u32::from(height);
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

async fn only_leader_produces_blocks(
    leader: &Node,
    non_leaders: &[Node],
    non_local_blocks_to_check: usize,
    block_import_timeout: Duration,
) {
    for _ in 0..non_local_blocks_to_check {
        tokio::time::timeout(block_import_timeout, wait_for_local_block(leader))
            .await
            .expect("Leader should import a local block");
        let mut follower_checks = non_leaders
            .iter()
            .map(|node| {
                tokio::time::timeout(
                    block_import_timeout,
                    wait_for_non_local_block_and_fail_on_local(node),
                )
            })
            .collect::<FuturesUnordered<_>>();
        while let Some(result) = follower_checks.next().await {
            result.expect("Non-leader should import a non-local block");
        }
    }
}

struct RedisTestServer {
    child: Option<Child>,
    port: u16,
    redis_url: String,
}

impl RedisTestServer {
    fn spawn() -> Self {
        let mut server = Self::new_stopped();
        server.start();
        server
    }

    fn new_stopped() -> Self {
        let port = bind_unused_port();
        Self {
            child: None,
            port,
            redis_url: format!("redis://127.0.0.1:{port}/"),
        }
    }

    fn start(&mut self) {
        if self.child.is_some() {
            return;
        }
        let child = spawn_redis_server(self.port);
        wait_for_redis_ready(self.port);
        self.child = Some(child);
    }

    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
    }

    fn redis_url(&self) -> String {
        self.redis_url.clone()
    }
}

impl Drop for RedisTestServer {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn bind_unused_port() -> u16 {
    let socket = TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("Should bind an ephemeral port");
    let port = socket.local_addr().expect("Should get local addr").port();
    drop(socket);
    port
}

fn spawn_redis_server(port: u16) -> Child {
    Command::new("redis-server")
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
        .expect("redis-server must be installed for this test")
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
