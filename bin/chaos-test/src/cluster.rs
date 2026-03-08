use std::time::Duration;

use fuel_core::{
    chain_config::ConsensusConfig,
    combined_database::CombinedDatabase,
    p2p_test_helpers::{Bootstrap, CustomizeConfig, make_config},
    service::{Config, FuelService, config::RedisLeaderLockConfig},
    state::rocks_db::DatabaseConfig,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::Input,
    fuel_types::Address,
    secrecy::Secret,
    signer::SignMode,
};
use rand::{SeedableRng, rngs::StdRng};
use tempfile::TempDir;
use tracing::info;

use crate::{
    proxy::{ProxyMode, TcpProxy},
    redis_server::RedisTestServer,
};

/// A running node with its service handle.
pub struct NodeHandle {
    pub service: FuelService,
}

pub struct Cluster {
    pub redis_servers: Vec<RedisTestServer>,
    /// proxies[node_idx][redis_idx]
    pub proxies: Vec<Vec<TcpProxy>>,
    pub nodes: Vec<Option<NodeHandle>>,
    node_configs: Vec<Config>,
    /// Persistent temp directories for each node's RocksDB — survive restarts
    node_data_dirs: Vec<TempDir>,
    node_count: usize,
}

impl Cluster {
    pub async fn new(
        node_count: usize,
        redis_count: usize,
        block_time: Duration,
        seed: u64,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);

        // 1. Start Redis servers
        info!("Starting {redis_count} Redis servers...");
        let mut redis_servers = Vec::with_capacity(redis_count);
        for i in 0..redis_count {
            let server = RedisTestServer::spawn();
            info!("  Redis {i} listening on port {}", server.port());
            redis_servers.push(server);
        }

        // 2. Create proxy grid: proxies[node_idx][redis_idx]
        info!("Creating {}x{} proxy grid...", node_count, redis_count);
        let mut proxies = Vec::with_capacity(node_count);
        for node_idx in 0..node_count {
            let mut node_proxies = Vec::with_capacity(redis_count);
            for redis_idx in 0..redis_count {
                let target =
                    format!("127.0.0.1:{}", redis_servers[redis_idx].port());
                let proxy = TcpProxy::start(target).await;
                info!(
                    "  Proxy node {node_idx} -> Redis {redis_idx}: port {}",
                    proxy.listen_port()
                );
                node_proxies.push(proxy);
            }
            proxies.push(node_proxies);
        }

        // 3. Generate signing key (shared across all nodes, same as leader_lock tests)
        let secret = SecretKey::random(&mut rng);
        let pub_key = Input::owner(&secret.public_key());

        // 4. Create bootstrap node
        let mut base_config = Config::local_node();
        update_signing_key(&mut base_config, pub_key);

        let bootstrap_config = make_config(
            "Bootstrap".to_string(),
            base_config.clone(),
            CustomizeConfig::no_overrides(),
        );
        let bootstrap = Bootstrap::new(&bootstrap_config).await.unwrap();
        let bootstrap_listeners = bootstrap.listeners();
        info!(
            "Bootstrap node started with {} listeners",
            bootstrap_listeners.len()
        );

        // 5. Create persistent temp directories and node configs
        let mut node_configs = Vec::with_capacity(node_count);
        let mut node_data_dirs = Vec::with_capacity(node_count);
        for node_idx in 0..node_count {
            let tmp_dir = TempDir::new().unwrap_or_else(|e| {
                panic!("Failed to create temp dir for node {node_idx}: {e}")
            });
            info!(
                "  Node {node_idx} data dir: {}",
                tmp_dir.path().display()
            );

            let redis_urls: Vec<String> = proxies[node_idx]
                .iter()
                .map(|p| p.listen_url())
                .collect();

            let leader_lock_config = RedisLeaderLockConfig {
                redis_urls,
                lease_key: format!("chaos-test:leader:{seed}"),
                lease_ttl: Duration::from_secs(2),
                node_timeout: Duration::from_millis(50),
                retry_delay: Duration::from_millis(100),
                max_retry_delay_offset: Duration::from_millis(25),
                max_attempts: 2,
                stream_max_len: 1000,
            };

            let mut node_config = make_config(
                format!("Node-{node_idx}"),
                base_config.clone(),
                CustomizeConfig::no_overrides(),
            );
            node_config.debug = true;
            node_config.block_production =
                Trigger::Interval { block_time };
            node_config.leader_lock = Some(leader_lock_config);
            node_config.consensus_signer =
                SignMode::Key(Secret::new(secret.into()));
            node_config.p2p.as_mut().unwrap().bootstrap_nodes =
                bootstrap_listeners.clone();
            node_config.p2p.as_mut().unwrap().reserved_nodes =
                bootstrap_listeners.clone();
            node_config.p2p.as_mut().unwrap().info_interval =
                Some(Duration::from_millis(100));
            node_config.min_connected_reserved_peers = 1;
            node_config.time_until_synced = block_time;

            node_configs.push(node_config);
            node_data_dirs.push(tmp_dir);
        }

        // 6. Start all nodes with persistent RocksDB
        info!("Starting {node_count} PoA nodes with persistent storage...");
        let mut nodes = Vec::with_capacity(node_count);
        for idx in 0..node_count {
            let handle = start_node_with_db(
                &node_data_dirs[idx],
                &node_configs[idx],
            )
            .await;
            info!("  Node {idx} started");
            nodes.push(Some(handle));
        }

        // Keep bootstrap alive by leaking it (cleaned up on process exit)
        std::mem::forget(bootstrap);

        Self {
            redis_servers,
            proxies,
            nodes,
            node_configs,
            node_data_dirs,
            node_count,
        }
    }

    pub fn is_node_alive(&self, idx: usize) -> bool {
        self.nodes.get(idx).is_some_and(|n| n.is_some())
    }

    pub fn is_redis_alive(&self, idx: usize) -> bool {
        self.redis_servers
            .get(idx)
            .is_some_and(|r| r.is_running())
    }

    pub fn alive_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    pub fn alive_redis_count(&self) -> usize {
        self.redis_servers.iter().filter(|r| r.is_running()).count()
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Count how many Redis instances a given node can reach
    /// (Redis must be alive AND the proxy must be healthy).
    pub fn reachable_redis_count(&self, node_idx: usize) -> usize {
        self.proxies[node_idx]
            .iter()
            .enumerate()
            .filter(|(redis_idx, proxy)| {
                self.is_redis_alive(*redis_idx) && proxy.is_healthy()
            })
            .count()
    }

    /// Returns true if at least one live node can reach a quorum of Redis.
    pub fn any_node_has_redis_quorum(&self) -> bool {
        let quorum = self.redis_servers.len() / 2 + 1;
        (0..self.node_count).any(|node_idx| {
            self.is_node_alive(node_idx)
                && self.reachable_redis_count(node_idx) >= quorum
        })
    }

    pub async fn stop_node(&mut self, idx: usize) {
        if let Some(handle) = self.nodes[idx].take() {
            info!("Stopping node {idx}");
            handle
                .service
                .send_stop_signal_and_await_shutdown()
                .await
                .ok();
        }
    }

    pub async fn restart_node(&mut self, idx: usize) {
        // Stop first if still alive
        self.stop_node(idx).await;

        info!("Restarting node {idx} (same data dir)");
        let handle = start_node_with_db(
            &self.node_data_dirs[idx],
            &self.node_configs[idx],
        )
        .await;
        self.nodes[idx] = Some(handle);
    }

    pub fn stop_redis(&mut self, idx: usize) {
        info!("Stopping Redis {idx}");
        self.redis_servers[idx].stop();
    }

    pub fn start_redis(&mut self, idx: usize) {
        info!("Starting Redis {idx}");
        self.redis_servers[idx].start();
    }

    pub fn set_proxy_mode(
        &self,
        node_idx: usize,
        redis_idx: usize,
        mode: ProxyMode,
    ) {
        self.proxies[node_idx][redis_idx].set_mode(mode);
    }

    pub fn restore_all_proxies(&self) {
        for node_proxies in &self.proxies {
            for proxy in node_proxies {
                proxy.set_mode(ProxyMode::Normal);
            }
        }
    }

    pub fn live_nodes(&self) -> Vec<(usize, &NodeHandle)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, n)| n.as_ref().map(|handle| (idx, handle)))
            .collect()
    }
}

/// Start a fuel-core node backed by a persistent RocksDB at `data_dir`.
/// On restart the same `data_dir` is reused, so all chain state survives.
async fn start_node_with_db(data_dir: &TempDir, config: &Config) -> NodeHandle {
    let database = CombinedDatabase::open(
        data_dir.path(),
        Default::default(), // StateRewindPolicy::NoRewind
        DatabaseConfig::config_for_tests(),
    )
    .unwrap_or_else(|e| {
        panic!(
            "Failed to open CombinedDatabase at {}: {e}",
            data_dir.path().display()
        )
    });

    let service = tokio::time::timeout(
        Duration::from_secs(120),
        FuelService::from_combined_database(database, config.clone()),
    )
    .await
    .unwrap_or_else(|_| panic!("Node should start in less than 120 seconds"))
    .expect("FuelService should start without error");

    NodeHandle { service }
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
    config.snapshot_reader =
        snapshot_reader.clone().with_chain_config(chain_config);
}
