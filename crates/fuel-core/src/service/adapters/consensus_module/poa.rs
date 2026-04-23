use crate::{
    fuel_core_graphql_api::ports::ConsensusModulePort,
    service::adapters::{
        BlockImporterAdapter,
        BlockProducerAdapter,
        P2PAdapter,
        PoAAdapter,
        TxPoolAdapter,
    },
};
use anyhow::anyhow;
use fuel_core_importer::ports::{
    BlockReconciliationWritePort,
    ImporterDatabase,
};
use fuel_core_metrics::poa_metrics::poa_metrics;
use fuel_core_poa::{
    ports::{
        BlockImporter,
        BlockReconciliationReadPort,
        LeaderState,
        P2pPort,
        PredefinedBlocks,
        TransactionPool,
        TransactionsSource,
    },
    service::{
        Mode,
        SharedState,
    },
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::{
        SealedBlock,
        block::Block,
        primitives::BlockId,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::{
            BlockImportInfo,
            UncommittedResult as UncommittedImporterResult,
        },
        executor::UncommittedResult,
    },
    tai64::Tai64,
};
use std::{
    collections::HashMap,
    path::{
        Path,
        PathBuf,
    },
    time::Duration,
};
use tokio::{
    sync::{
        Mutex,
        watch,
    },
    time::{
        Instant,
        sleep,
        timeout,
    },
};
use tokio_stream::{
    StreamExt,
    wrappers::BroadcastStream,
};
use tracing::error;

pub mod pre_confirmation_signature;

const CHECK_LEASE_OWNER_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/check_lease_owner.lua"
));

const RELEASE_LOCK_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/release_lock.lua"
));

const PROMOTE_LEADER_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/promote_leader.lua"
));

const WRITE_BLOCK_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/write_block.lua"
));

const READ_STREAM_ENTRIES_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/read_stream_entries.lua"
));

const READ_LATEST_STREAM_ENTRY_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/redis_leader_lease_adapter_scripts/read_latest_stream_entry.lua"
));

struct RedisNode {
    redis_client: redis::Client,
    cached_connection: Mutex<Option<redis::aio::MultiplexedConnection>>,
}

impl Clone for RedisNode {
    fn clone(&self) -> Self {
        Self {
            redis_client: self.redis_client.clone(),
            cached_connection: Mutex::new(None),
        }
    }
}

pub struct RedisLeaderLeaseAdapter {
    redis_nodes: Vec<RedisNode>,
    quorum: usize,
    quorum_disruption_budget: u32,
    lease_key: String,
    epoch_key: String,
    block_stream_key: String,
    lease_owner_token: String,
    drop_release_guard: std::sync::Arc<()>,
    current_epoch_token: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    lease_ttl_millis: u64,
    lease_drift_millis: u64,
    node_timeout: Duration,
    retry_delay_millis: u64,
    max_retry_delay_offset_millis: u64,
    max_attempts: usize,
    stream_max_len: u32,
}

impl Clone for RedisLeaderLeaseAdapter {
    fn clone(&self) -> Self {
        Self {
            redis_nodes: self.redis_nodes.clone(),
            quorum: self.quorum,
            quorum_disruption_budget: self.quorum_disruption_budget,
            lease_key: self.lease_key.clone(),
            epoch_key: self.epoch_key.clone(),
            block_stream_key: self.block_stream_key.clone(),
            lease_owner_token: self.lease_owner_token.clone(),
            drop_release_guard: self.drop_release_guard.clone(),
            current_epoch_token: self.current_epoch_token.clone(),
            lease_ttl_millis: self.lease_ttl_millis,
            lease_drift_millis: self.lease_drift_millis,
            node_timeout: self.node_timeout,
            retry_delay_millis: self.retry_delay_millis,
            max_retry_delay_offset_millis: self.max_retry_delay_offset_millis,
            max_attempts: self.max_attempts,
            stream_max_len: self.stream_max_len,
        }
    }
}

#[derive(Default, Clone)]
pub struct NoopReconciliationAdapter;

#[allow(clippy::large_enum_variant)]
pub enum ReconciliationAdapter {
    Redis(RedisLeaderLeaseAdapter),
    Noop(NoopReconciliationAdapter),
}

impl RedisLeaderLeaseAdapter {
    fn calculate_quorum(redis_nodes_len: usize, quorum_disruption_budget: u32) -> usize {
        let majority = redis_nodes_len
            .checked_div(2)
            .unwrap_or(0)
            .saturating_add(1);
        let disruption_budget = usize::try_from(quorum_disruption_budget).unwrap_or(0);
        majority
            .saturating_add(disruption_budget)
            .min(redis_nodes_len)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        redis_urls: Vec<String>,
        lease_key: String,
        lease_ttl: Duration,
        node_timeout: Duration,
        retry_delay: Duration,
        max_retry_delay_offset: Duration,
        max_attempts: u32,
        stream_max_len: u32,
    ) -> anyhow::Result<Self> {
        let redis_nodes = redis_urls
            .into_iter()
            .map(|redis_url| {
                redis::Client::open(redis_url).map(|redis_client| RedisNode {
                    redis_client,
                    cached_connection: Mutex::new(None),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if redis_nodes.is_empty() {
            return Err(anyhow!(
                "At least one redis url is required for leader lock"
            ));
        }
        let quorum_disruption_budget = 0u32;
        let quorum = Self::calculate_quorum(redis_nodes.len(), quorum_disruption_budget);
        let lease_ttl_millis = u64::try_from(lease_ttl.as_millis())?;
        let retry_delay_millis = u64::try_from(retry_delay.as_millis())?;
        let max_retry_delay_offset_millis =
            u64::try_from(max_retry_delay_offset.as_millis())?;
        let max_attempts = usize::try_from(max_attempts)?.max(1);
        let lease_owner_token = uuid::Uuid::new_v4().to_string();
        let epoch_key = format!("{lease_key}:epoch:token");
        let block_stream_key = format!("{lease_key}:block:stream");
        let lease_drift_millis = lease_ttl_millis
            .checked_div(100)
            .unwrap_or(0)
            .saturating_add(2);
        Ok(Self {
            redis_nodes,
            quorum,
            quorum_disruption_budget,
            lease_key,
            epoch_key,
            block_stream_key,
            lease_owner_token,
            drop_release_guard: std::sync::Arc::new(()),
            current_epoch_token: std::sync::Arc::new(std::sync::Mutex::new(None)),
            lease_ttl_millis,
            lease_drift_millis,
            node_timeout,
            retry_delay_millis,
            max_retry_delay_offset_millis,
            max_attempts,
            stream_max_len,
        })
    }

    pub fn with_quorum_disruption_budget(
        mut self,
        quorum_disruption_budget: u32,
    ) -> Self {
        self.quorum_disruption_budget = quorum_disruption_budget;
        self.quorum =
            Self::calculate_quorum(self.redis_nodes.len(), quorum_disruption_budget);
        self
    }

    async fn multiplexed_connection(
        &self,
        redis_node: &RedisNode,
    ) -> anyhow::Result<redis::aio::MultiplexedConnection> {
        if let Some(connection) =
            redis_node.cached_connection.lock().await.as_ref().cloned()
        {
            return Ok(connection);
        }

        let new_connection = timeout(
            self.node_timeout,
            redis_node.redis_client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| anyhow!("Timed out while connecting to redis leader-lock node"))??;
        let mut cached_connection = redis_node.cached_connection.lock().await;
        if let Some(connection) = cached_connection.as_ref().cloned() {
            return Ok(connection);
        }
        *cached_connection = Some(new_connection.clone());
        Ok(new_connection)
    }

    async fn clear_cached_connection(&self, redis_node: &RedisNode) {
        let mut cached_connection = redis_node.cached_connection.lock().await;
        *cached_connection = None;
        poa_metrics().connection_reset_total.inc();
    }

    async fn check_lease_owner_on_node(&self, redis_node: &RedisNode) -> bool {
        let mut connection = match self.multiplexed_connection(redis_node).await {
            Ok(connection) => connection,
            Err(_) => return false,
        };
        let is_owner = timeout(
            self.node_timeout,
            redis::Script::new(CHECK_LEASE_OWNER_SCRIPT)
                .key(&self.lease_key)
                .arg(&self.lease_owner_token)
                .invoke_async::<i32>(&mut connection),
        )
        .await;
        match is_owner {
            Ok(Ok(is_owner)) => is_owner == 1,
            Err(_) => {
                self.clear_cached_connection(redis_node).await;
                false
            }
            Ok(Err(_)) => {
                self.clear_cached_connection(redis_node).await;
                false
            }
        }
    }

    async fn promote_leader_on_node(
        &self,
        redis_node: &RedisNode,
    ) -> anyhow::Result<Option<u64>> {
        let mut connection = match self.multiplexed_connection(redis_node).await {
            Ok(connection) => connection,
            Err(_) => return Ok(None),
        };
        let promoted = timeout(
            self.node_timeout,
            redis::Script::new(PROMOTE_LEADER_SCRIPT)
                .key(&self.lease_key)
                .key(&self.epoch_key)
                .arg(&self.lease_owner_token)
                .arg(self.lease_ttl_millis)
                .invoke_async::<u64>(&mut connection),
        )
        .await;
        match promoted {
            Ok(Ok(token)) => Ok(Some(token)),
            Ok(Err(err)) => {
                if err.to_string().contains("LOCK_HELD:") {
                    return Ok(None);
                }
                self.clear_cached_connection(redis_node).await;
                Ok(None)
            }
            Err(_) => {
                self.clear_cached_connection(redis_node).await;
                Ok(None)
            }
        }
    }

    async fn release_lease_on_node(&self, redis_node: &RedisNode) -> bool {
        let mut connection = match self.multiplexed_connection(redis_node).await {
            Ok(connection) => connection,
            Err(_) => return false,
        };
        let released = timeout(
            self.node_timeout,
            redis::Script::new(RELEASE_LOCK_SCRIPT)
                .key(&self.lease_key)
                .arg(&self.lease_owner_token)
                .invoke_async::<i32>(&mut connection),
        )
        .await;
        match released {
            Ok(Ok(released)) => released == 1,
            Err(_) => {
                self.clear_cached_connection(redis_node).await;
                false
            }
            Ok(Err(_)) => {
                self.clear_cached_connection(redis_node).await;
                false
            }
        }
    }

    fn quorum_reached(&self, success_count: usize) -> bool {
        success_count >= self.quorum
    }

    fn calculate_remaining_validity_millis(&self, elapsed_millis: u64) -> u64 {
        self.lease_ttl_millis
            .saturating_sub(elapsed_millis.saturating_add(self.lease_drift_millis))
    }

    fn random_retry_delay_offset_millis(&self) -> u64 {
        if self.max_retry_delay_offset_millis == 0 {
            return 0;
        }
        rand::random::<u64>()
            .checked_rem(self.max_retry_delay_offset_millis.saturating_add(1))
            .unwrap_or(0)
    }

    async fn release_lease_on_all_nodes(&self) {
        let _ = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.release_lease_on_node(redis_node)),
        )
        .await;
    }

    async fn delay_next_retry(&self) {
        let retry_delay_millis = self
            .retry_delay_millis
            .saturating_add(self.random_retry_delay_offset_millis());
        sleep(Duration::from_millis(retry_delay_millis)).await;
    }

    async fn has_lease_owner_quorum(&self) -> anyhow::Result<bool> {
        let ownership = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.check_lease_owner_on_node(redis_node)),
        )
        .await;
        let owner_count = ownership.iter().filter(|&&is_owner| is_owner).count();
        if !self.quorum_reached(owner_count) {
            return Ok(false);
        }

        // Best-effort: acquire the lock on nodes we don't own yet.
        // Expands write coverage beyond minimum quorum so block data
        // is replicated to more nodes, improving fault tolerance.
        // If a newly-acquired node returns a higher epoch (from
        // election storm drift), adopt it so write_block.lua uses
        // a consistent epoch across all owned nodes.
        let non_owned: Vec<&RedisNode> = self
            .redis_nodes
            .iter()
            .zip(ownership.iter())
            .filter(|(_, is_owner)| !**is_owner)
            .map(|(node, _)| node)
            .collect();

        if !non_owned.is_empty() {
            let results = futures::future::join_all(
                non_owned
                    .into_iter()
                    .map(|redis_node| self.promote_leader_on_node(redis_node)),
            )
            .await;

            if let Some(max_new) =
                results.into_iter().filter_map(|r| r.ok().flatten()).max()
                && let Ok(mut epoch) = self.current_epoch_token.lock()
            {
                let current = epoch.unwrap_or(0);
                if max_new > current {
                    tracing::debug!(
                        old_epoch = current,
                        new_epoch = max_new,
                        "Adopted higher epoch from lock expansion"
                    );
                    *epoch = Some(max_new);
                }
            }
        }

        Ok(true)
    }

    async fn acquire_lease_if_free(&self) -> anyhow::Result<bool> {
        let promotion_start = std::time::Instant::now();
        for attempt_index in 0..self.max_attempts {
            let start = std::time::Instant::now();
            let promoted_nodes = futures::future::join_all(
                self.redis_nodes
                    .iter()
                    .map(|redis_node| self.promote_leader_on_node(redis_node)),
            )
            .await;
            let promoted_tokens = promoted_nodes
                .into_iter()
                .filter_map(|token| token.ok().flatten())
                .collect::<Vec<_>>();
            let acquired_count = promoted_tokens.len();
            let elapsed_millis =
                u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            let validity_millis =
                self.calculate_remaining_validity_millis(elapsed_millis);
            if self.quorum_reached(acquired_count) && validity_millis > 0 {
                // Record epoch drift across quorum nodes
                if promoted_tokens.len() > 1
                    && let (Some(min_tok), Some(max_tok)) = (
                        promoted_tokens.iter().copied().min(),
                        promoted_tokens.iter().copied().max(),
                    )
                {
                    poa_metrics().epoch_max_drift.set(
                        i64::try_from(max_tok.saturating_sub(min_tok))
                            .unwrap_or(i64::MAX),
                    );
                }
                if let Some(max_token) = promoted_tokens.into_iter().max() {
                    let mut current_epoch_token = self
                        .current_epoch_token
                        .lock()
                        .map_err(|e| anyhow!("epoch token lock poisoned: {}", e))?;
                    *current_epoch_token = Some(max_token);
                    poa_metrics()
                        .leader_epoch
                        .set(i64::try_from(max_token).unwrap_or(i64::MAX));
                }
                poa_metrics().promotion_success_total.inc();
                poa_metrics()
                    .promotion_duration_s
                    .observe(promotion_start.elapsed().as_secs_f64());
                return Ok(true);
            }
            self.release_lease_on_all_nodes().await;
            let is_last_attempt = attempt_index.saturating_add(1) == self.max_attempts;
            if !is_last_attempt {
                self.delay_next_retry().await;
            }
        }
        poa_metrics().promotion_failure_total.inc();
        poa_metrics()
            .promotion_duration_s
            .observe(promotion_start.elapsed().as_secs_f64());
        Ok(false)
    }

    async fn release_lease_on_client(
        redis_client: redis::Client,
        lease_key: String,
        lease_owner_token: String,
        node_timeout: Duration,
    ) {
        let connection = timeout(
            node_timeout,
            redis_client.get_multiplexed_async_connection(),
        )
        .await;
        let mut connection = match connection {
            Ok(Ok(connection)) => connection,
            Err(_) => return,
            Ok(Err(_)) => return,
        };
        let _ = timeout(
            node_timeout,
            redis::Script::new(RELEASE_LOCK_SCRIPT)
                .key(lease_key)
                .arg(lease_owner_token)
                .invoke_async::<i32>(&mut connection),
        )
        .await;
    }

    async fn release_lease_on_clients(
        redis_clients: Vec<redis::Client>,
        lease_key: String,
        lease_owner_token: String,
        node_timeout: Duration,
    ) {
        let _ =
            futures::future::join_all(redis_clients.into_iter().map(|redis_client| {
                Self::release_lease_on_client(
                    redis_client,
                    lease_key.clone(),
                    lease_owner_token.clone(),
                    node_timeout,
                )
            }))
            .await;
    }

    fn release_lease_on_clients_sync(
        redis_clients: Vec<redis::Client>,
        lease_key: String,
        lease_owner_token: String,
    ) {
        redis_clients.into_iter().for_each(|redis_client| {
            let Ok(mut connection) = redis_client.get_connection() else {
                return;
            };
            let _ = redis::Script::new(RELEASE_LOCK_SCRIPT)
                .key(&lease_key)
                .arg(&lease_owner_token)
                .invoke::<i32>(&mut connection);
        });
    }

    async fn read_latest_stream_entry_on_node(
        &self,
        redis_node: &RedisNode,
    ) -> anyhow::Result<Option<(u32, String)>> {
        let mut connection = self.multiplexed_connection(redis_node).await?;
        let latest_entry = timeout(
            self.node_timeout,
            redis::Script::new(READ_LATEST_STREAM_ENTRY_SCRIPT)
                .key(&self.block_stream_key)
                .invoke_async::<Vec<String>>(&mut connection),
        )
        .await;
        match latest_entry {
            Err(_) => {
                self.clear_cached_connection(redis_node).await;
                Err(anyhow!(
                    "Timed out reading latest stream entry from Redis node"
                ))
            }
            Ok(Err(e)) => {
                self.clear_cached_connection(redis_node).await;
                Err(anyhow!(
                    "Failed to read latest stream entry from Redis node: {e}"
                ))
            }
            Ok(Ok(entry)) => {
                if entry.len() != 2 {
                    return Ok(None);
                }
                let height = entry[0]
                    .parse::<u32>()
                    .map_err(|e| anyhow!("Invalid latest stream entry height: {e}"))?;
                Ok(Some((height, entry[1].clone())))
            }
        }
    }

    async fn should_reconcile_from_stream(
        &self,
        next_height: BlockHeight,
    ) -> anyhow::Result<bool> {
        let next_height = u32::from(next_height);
        let latest_results = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.read_latest_stream_entry_on_node(redis_node)),
        )
        .await;
        let mut successful_reads = 0usize;
        let mut failed_count = 0usize;
        let mut nodes_indicating_backlog = 0usize;
        for result in latest_results {
            match result {
                Ok(Some((latest_height, _latest_stream_id))) => {
                    successful_reads = successful_reads.saturating_add(1);
                    if latest_height >= next_height {
                        nodes_indicating_backlog =
                            nodes_indicating_backlog.saturating_add(1);
                    }
                }
                Ok(None) => {
                    successful_reads = successful_reads.saturating_add(1);
                }
                Err(e) => {
                    tracing::warn!("Redis latest stream read failed: {e}");
                    failed_count = failed_count.saturating_add(1);
                }
            }
        }
        if !self.quorum_reached(successful_reads) {
            return Err(anyhow!(
                "Cannot reconcile: only {}/{} Redis nodes responded ({} failed)",
                successful_reads,
                self.redis_nodes.len(),
                failed_count
            ));
        }
        Ok(nodes_indicating_backlog > 0)
    }

    async fn read_stream_entries_on_node(
        &self,
        redis_node: &RedisNode,
        next_height: u32,
        max_entries: usize,
    ) -> anyhow::Result<Vec<(u32, u64, SealedBlock)>> {
        if max_entries == 0 {
            return Ok(Vec::new());
        }

        let mut connection = self.multiplexed_connection(redis_node).await?;
        let count = u32::try_from(max_entries).unwrap_or(u32::MAX);
        let stream_entries = timeout(
            self.node_timeout,
            redis::Script::new(READ_STREAM_ENTRIES_SCRIPT)
                .key(&self.block_stream_key)
                .arg(next_height)
                .arg(count)
                .invoke_async::<Vec<(u32, u64, Vec<u8>, String)>>(&mut connection),
        )
        .await;

        let entries = match stream_entries {
            Err(_) => {
                self.clear_cached_connection(redis_node).await;
                return Err(anyhow!("Timed out reading stream entries from Redis node"));
            }
            Ok(Err(e)) => {
                self.clear_cached_connection(redis_node).await;
                return Err(anyhow!(
                    "Failed to read stream entries from Redis node: {e}"
                ));
            }
            Ok(Ok(entries)) => entries,
        };

        let mut blocks = Vec::new();
        for (height, epoch, bytes, _stream_id) in entries {
            match postcard::from_bytes::<SealedBlock>(&bytes) {
                Ok(block) => blocks.push((height, epoch, block)),
                Err(e) => {
                    tracing::warn!(
                        "Skipping stream entry: failed to deserialize block at height {height}: {e}"
                    );
                }
            }
        }

        Ok(blocks)
    }

    async fn unreconciled_blocks(
        &self,
        next_height: BlockHeight,
    ) -> anyhow::Result<Vec<SealedBlock>> {
        if !self.should_reconcile_from_stream(next_height).await? {
            return Ok(Vec::new());
        }
        let mut reconciled = Vec::new();
        let max_reconcile_blocks_per_round =
            usize::try_from(self.stream_max_len).unwrap_or(usize::MAX);
        let next_height_u32 = u32::from(next_height);
        let read_results =
            futures::future::join_all(self.redis_nodes.iter().map(|redis_node| {
                self.read_stream_entries_on_node(
                    redis_node,
                    next_height_u32,
                    max_reconcile_blocks_per_round,
                )
            }))
            .await;

        let mut successful_reads = Vec::new();
        let mut failed_count = 0usize;
        for result in read_results {
            match result {
                Ok(entries) => successful_reads.push(entries),
                Err(e) => {
                    tracing::warn!("Redis stream read failed: {e}");
                    failed_count = failed_count.saturating_add(1);
                }
            }
        }

        if !self.quorum_reached(successful_reads.len()) {
            return Err(anyhow!(
                "Cannot reconcile: only {}/{} Redis nodes responded ({} failed)",
                successful_reads.len(),
                self.redis_nodes.len(),
                failed_count
            ));
        }

        let blocks_by_node = successful_reads
            .into_iter()
            .map(|entries| {
                entries.into_iter().fold(
                    HashMap::<u32, HashMap<u64, SealedBlock>>::new(),
                    |mut blocks_by_height, (height, epoch, block)| {
                        blocks_by_height
                            .entry(height)
                            .or_default()
                            .insert(epoch, block);
                        blocks_by_height
                    },
                )
            })
            .collect::<Vec<_>>();

        // Compute stream trim headroom: min stream height - local committed height
        let min_stream_height = blocks_by_node
            .iter()
            .flat_map(|blocks_by_height| blocks_by_height.keys().copied())
            .min();
        if let Some(min_h) = min_stream_height {
            let local_committed = i64::from(u32::from(next_height).saturating_sub(1));
            let headroom = i64::from(min_h).saturating_sub(local_committed);
            poa_metrics().stream_trim_headroom.set(headroom);
        }

        let mut current_height = u32::from(next_height);

        for _ in 0..max_reconcile_blocks_per_round {
            let nodes_with_height = blocks_by_node
                .iter()
                .filter(|blocks_by_height| blocks_by_height.contains_key(&current_height))
                .count();

            tracing::debug!(
                "unreconciled_blocks: height={current_height} nodes_with_height={nodes_with_height}/{}",
                blocks_by_node.len()
            );

            if nodes_with_height == 0 {
                if reconciled.is_empty() {
                    return Err(anyhow!(
                        "Backlog unresolved at height {current_height}: \
                         stream indicates backlog but no entries found at next height"
                    ));
                }
                break;
            }

            // Group votes by block_id only (not epoch). The same block can
            // be written to different nodes with different epochs during
            // re-promotion storms — but if the block_id matches, it's the
            // same block and all copies count toward quorum. We track the
            // max epoch per block_id as the tiebreaker for fork resolution
            // when block_ids genuinely differ.
            let votes = blocks_by_node
                .iter()
                .filter_map(|blocks_by_height| blocks_by_height.get(&current_height))
                .flat_map(|blocks_by_epoch| blocks_by_epoch.iter())
                .fold(
                    HashMap::<BlockId, (u64, usize, SealedBlock)>::new(),
                    |mut votes, (epoch, block)| {
                        let vote_key = block.entity.id();
                        match votes.get_mut(&vote_key) {
                            Some((max_epoch, count, _)) => {
                                *count = count.saturating_add(1);
                                if *epoch > *max_epoch {
                                    *max_epoch = *epoch;
                                }
                            }
                            None => {
                                votes.insert(vote_key, (*epoch, 1, block.clone()));
                            }
                        }
                        votes
                    },
                );

            let winner = votes
                .into_iter()
                .max_by_key(|(_, (max_epoch, _, _))| *max_epoch)
                .map(|(_, (_, count, block))| (count, block));

            if let Some((count, block)) = winner {
                if self.quorum_reached(count) {
                    // Block already has quorum — reconcile it directly
                    reconciled.push(block);
                } else {
                    // Sub-quorum block: repropose to all nodes to reach quorum.
                    // This repairs orphaned partial writes from failed leaders.
                    // HEIGHT_EXISTS on nodes that already have the block returns
                    // Ok(false), and nodes missing it accept the write.
                    tracing::info!(
                        "Repairing sub-quorum block at height {current_height} \
                         (found on {count}/{} nodes)",
                        blocks_by_node.len()
                    );
                    match self.repair_sub_quorum_block(&block, count) {
                        Ok(true) => {
                            tracing::info!(
                                "Repair succeeded — block at height {current_height} \
                                 now has quorum"
                            );
                            reconciled.push(block);
                        }
                        Ok(false) => {
                            tracing::warn!(
                                "Repair failed to reach quorum at height \
                                 {current_height} — will retry next round"
                            );
                            if reconciled.is_empty() {
                                return Err(anyhow!(
                                    "Backlog unresolved at height {current_height}: \
                                     repair failed to reach quorum"
                                ));
                            }
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Repair error at height {current_height}: {e}"
                            );
                            if reconciled.is_empty() {
                                return Err(anyhow!(
                                    "Backlog unresolved at height {current_height}: \
                                     repair error: {e}"
                                ));
                            }
                            break;
                        }
                    }
                }
            } else {
                if reconciled.is_empty() {
                    return Err(anyhow!(
                        "Backlog unresolved at height {current_height}: \
                         no winning block candidate"
                    ));
                }
                break;
            }

            let Some(next) = current_height.checked_add(1) else {
                break;
            };
            current_height = next;
        }

        Ok(reconciled)
    }

    async fn can_produce_block(&self) -> anyhow::Result<bool> {
        tracing::debug!("Checking Redis leader lock");
        if self.has_lease_owner_quorum().await? {
            return Ok(true);
        }
        self.acquire_lease_if_free().await
    }

    async fn release_if_owner(&self) -> anyhow::Result<()> {
        tracing::debug!("Releasing Redis leader lock");
        if !self.has_lease_owner_quorum().await? {
            let mut current_epoch_token = self
                .current_epoch_token
                .lock()
                .map_err(|_| anyhow!("cannot access epoch token, poisoned lock"))?;
            *current_epoch_token = None;
            return Ok(());
        }

        let releases = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.release_lease_on_node(redis_node)),
        )
        .await;
        let released_count = releases.into_iter().filter(|released| *released).count();
        if self.quorum_reached(released_count) {
            let mut current_epoch_token = self
                .current_epoch_token
                .lock()
                .map_err(|_| anyhow!("cannot access epoch token, poisoned lock"))?;
            *current_epoch_token = None;
            Ok(())
        } else {
            Err(anyhow!("Failed to release lease on quorum"))
        }
    }

    fn publish_block_on_all_nodes(
        &self,
        epoch: u64,
        block: &SealedBlock,
        block_data: &[u8],
    ) -> Vec<anyhow::Result<WriteBlockResult>> {
        // Detached `std::thread::spawn` (not scoped) lets us return as soon
        // as `Written` quorum is reached without waiting for slow nodes.
        // Stragglers — including any thread blocked inside the sync `redis`
        // client — are abandoned: they will exit when their syscall returns
        // (e.g. when the peer recovers, or when SO_RCVTIMEO fires inside
        // `invoke_write_block_script`'s set_read_timeout/set_write_timeout
        // window). Their result is discarded via the dropped channel.
        //
        // This is the class-of-failure fix for the 2026-04-22 mainnet hang
        // where a half-alive ElastiCache node stalled one thread inside the
        // pre-1.2 `redis::Client::get_connection_with_timeout` handshake,
        // which combined with the previous `std::thread::scope` to wedge
        // every block publish forever. The redis crate upgrade to 1.2 also
        // closes that specific upstream bug; the short-circuit here protects
        // against any future single-node hang that survives per-syscall
        // timeouts.
        let n = self.redis_nodes.len();
        let block_data: std::sync::Arc<[u8]> = block_data.into();
        let block_height = u32::from(*block.entity.header().height());
        let block_stream_key: std::sync::Arc<str> = self.block_stream_key.as_str().into();
        let epoch_key: std::sync::Arc<str> = self.epoch_key.as_str().into();
        let lease_key: std::sync::Arc<str> = self.lease_key.as_str().into();
        let lease_owner_token: std::sync::Arc<str> =
            self.lease_owner_token.as_str().into();
        let lease_ttl_millis = self.lease_ttl_millis;
        let stream_max_len = self.stream_max_len;
        let node_timeout = self.node_timeout;

        let (tx, rx) =
            std::sync::mpsc::channel::<(usize, anyhow::Result<WriteBlockResult>)>();

        for (idx, redis_node) in self.redis_nodes.iter().enumerate() {
            let client = redis_node.redis_client.clone();
            let tx = tx.clone();
            let block_data = block_data.clone();
            let block_stream_key = block_stream_key.clone();
            let epoch_key = epoch_key.clone();
            let lease_key = lease_key.clone();
            let lease_owner_token = lease_owner_token.clone();
            std::thread::spawn(move || {
                let result = Self::invoke_write_block_script(
                    &client,
                    node_timeout,
                    &block_stream_key,
                    &epoch_key,
                    &lease_key,
                    epoch,
                    &lease_owner_token,
                    block_height,
                    &block_data,
                    lease_ttl_millis,
                    stream_max_len,
                );
                // Send may fail if the receiver was dropped because we
                // already short-circuited on quorum — that's intentional.
                let _ = tx.send((idx, result));
            });
        }
        // Drop our local sender so `rx.recv` returns `Err` once every spawned
        // thread has either sent or dropped its sender.
        drop(tx);

        let mut results: Vec<Option<anyhow::Result<WriteBlockResult>>> =
            (0..n).map(|_| None).collect();
        let mut written_count = 0usize;
        let mut received = 0usize;

        while received < n {
            let Ok((idx, result)) = rx.recv() else {
                // All senders dropped (every thread either reported or
                // panicked). Stop draining and fill remaining slots below.
                break;
            };
            if matches!(result, Ok(WriteBlockResult::Written)) {
                written_count = written_count.saturating_add(1);
            }
            results[idx] = Some(result);
            received = received.saturating_add(1);

            if self.quorum_reached(written_count) {
                // Quorum reached. Return immediately; any threads still
                // running are abandoned. Their later `tx.send` will fail
                // silently because we drop the receiver below.
                break;
            }
        }

        results
            .into_iter()
            .map(|r| {
                r.unwrap_or_else(|| {
                    Err(anyhow!(
                        "publish abandoned: quorum reached before this \
                         node responded"
                    ))
                })
            })
            .collect()
    }

    /// `'static` helper that runs `write_block.lua` against a single node.
    /// Free function (no `&self`) so detached threads spawned by
    /// `publish_block_on_all_nodes` can run it without borrowing the adapter.
    #[allow(clippy::too_many_arguments)]
    fn invoke_write_block_script(
        redis_client: &redis::Client,
        node_timeout: Duration,
        block_stream_key: &str,
        epoch_key: &str,
        lease_key: &str,
        epoch: u64,
        lease_owner_token: &str,
        block_height: u32,
        block_data: &[u8],
        lease_ttl_millis: u64,
        stream_max_len: u32,
    ) -> anyhow::Result<WriteBlockResult> {
        let mut connection = redis_client.get_connection_with_timeout(node_timeout)?;
        connection.set_read_timeout(Some(node_timeout))?;
        connection.set_write_timeout(Some(node_timeout))?;
        let lua_start = std::time::Instant::now();
        let write_result = redis::Script::new(WRITE_BLOCK_SCRIPT)
            .key(block_stream_key)
            .key(epoch_key)
            .key(lease_key)
            .arg(epoch)
            .arg(lease_owner_token)
            .arg(block_height)
            .arg(block_data)
            .arg(lease_ttl_millis)
            .arg(stream_max_len)
            .invoke::<String>(&mut connection);
        poa_metrics()
            .write_block_duration_s
            .observe(lua_start.elapsed().as_secs_f64());
        match write_result {
            Ok(_) => {
                poa_metrics().write_block_success_total.inc();
                Ok(WriteBlockResult::Written)
            }
            Err(err) if err.to_string().contains("HEIGHT_EXISTS:") => {
                poa_metrics().write_block_height_exists_total.inc();
                tracing::debug!(
                    "write_block: height already exists (height={block_height})"
                );
                Ok(WriteBlockResult::HeightExists)
            }
            Err(err) if err.to_string().contains("FENCING_ERROR:") => {
                poa_metrics().write_block_fencing_error_total.inc();
                tracing::warn!(
                    "write_block: fencing rejected (height={block_height}): {err}"
                );
                Ok(WriteBlockResult::FencingRejected)
            }
            Err(err) => {
                poa_metrics().write_block_error_total.inc();
                Err(err.into())
            }
        }
    }

    /// Repropose a sub-quorum block to all Redis nodes to reach quorum.
    /// Called during reconciliation when a block exists on some nodes but
    /// below quorum — possibly from a leader that published and committed
    /// locally but whose write only reached a subset of nodes.
    ///
    /// `pre_existing_count` is the number of nodes already confirmed to
    /// have this specific block during the reconciliation read phase.
    ///
    /// Uses `publish_block_on_all_nodes` which runs `write_block.lua`:
    /// - Written: node accepted the block (counted toward quorum)
    /// - HEIGHT_EXISTS: node has *some* block at this height — may be a
    ///   different block from a competing partial write, so NOT counted
    /// - FENCING_ERROR: lost the lock — abort the repair
    /// - The total (pre_existing + newly written) must reach quorum
    fn repair_sub_quorum_block(
        &self,
        block: &SealedBlock,
        pre_existing_count: usize,
    ) -> anyhow::Result<bool> {
        let epoch = match *self
            .current_epoch_token
            .lock()
            .map_err(|_| anyhow!("cannot access epoch token, poisoned lock"))?
        {
            Some(epoch) => epoch,
            None => {
                return Err(anyhow!(
                    "Cannot repair block because fencing token is not initialized"
                ));
            }
        };
        let block_data = postcard::to_allocvec(block)?;
        // Start from the pre-existing count (nodes already confirmed to
        // have this specific block during reconciliation). Only count
        // newly Written nodes — HeightExists means the node has *some*
        // block at this height, but it might be a different block from
        // a competing leader's partial write.
        let mut total_with_block = pre_existing_count;
        for result in self.publish_block_on_all_nodes(epoch, block, &block_data) {
            match result {
                Ok(WriteBlockResult::Written) => {
                    total_with_block = total_with_block.saturating_add(1);
                }
                Ok(WriteBlockResult::HeightExists) => {
                    // Node has some block at this height — may or may
                    // not be ours. Don't count it; the pre_existing_count
                    // already includes nodes confirmed to have our block.
                }
                Ok(WriteBlockResult::FencingRejected) => {
                    // Lost the lock — repair is invalid, abort
                    return Err(anyhow!(
                        "Lost lock during repair — another leader took over"
                    ));
                }
                Err(err) => {
                    tracing::debug!("Repair write to node failed: {err}");
                }
            }
        }
        let reached_quorum = self.quorum_reached(total_with_block);
        if reached_quorum {
            poa_metrics().repair_success_total.inc();
        } else {
            poa_metrics().repair_failure_total.inc();
        }
        Ok(reached_quorum)
    }
}

/// Result of a `write_block.lua` invocation on a single Redis node.
enum WriteBlockResult {
    /// Block was successfully written to the stream.
    Written,
    /// A block at this height already exists in the stream.
    HeightExists,
    /// Lock lost or epoch is stale — another leader holds the lock.
    FencingRejected,
}

impl PoAAdapter {
    pub fn new(shared_state: Option<SharedState>) -> Self {
        Self { shared_state }
    }

    pub async fn manually_produce_blocks(
        &self,
        start_time: Option<Tai64>,
        mode: Mode,
    ) -> anyhow::Result<()> {
        self.shared_state
            .as_ref()
            .ok_or(anyhow!("The block production is disabled"))?
            .manually_produce_block(start_time, mode)
            .await
    }
}

#[async_trait::async_trait]
impl BlockReconciliationReadPort for NoopReconciliationAdapter {
    async fn leader_state(
        &self,
        _next_height: BlockHeight,
    ) -> anyhow::Result<LeaderState> {
        Ok(LeaderState::ReconciledLeader)
    }

    async fn release(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl BlockReconciliationReadPort for RedisLeaderLeaseAdapter {
    async fn leader_state(
        &self,
        next_height: BlockHeight,
    ) -> anyhow::Result<LeaderState> {
        if self.can_produce_block().await? {
            poa_metrics().is_leader.set(1);
            if let Ok(epoch) = self.current_epoch_token.lock()
                && let Some(epoch) = *epoch
            {
                poa_metrics()
                    .leader_epoch
                    .set(i64::try_from(epoch).unwrap_or(i64::MAX));
            }
            let reconcile_start = std::time::Instant::now();
            let unreconciled_blocks = self.unreconciled_blocks(next_height).await?;
            poa_metrics()
                .reconciliation_duration_s
                .observe(reconcile_start.elapsed().as_secs_f64());
            if unreconciled_blocks.is_empty() {
                Ok(LeaderState::ReconciledLeader)
            } else {
                Ok(LeaderState::UnreconciledBlocks(unreconciled_blocks))
            }
        } else {
            poa_metrics().is_leader.set(0);
            Ok(LeaderState::ReconciledFollower)
        }
    }

    async fn release(&self) -> anyhow::Result<()> {
        self.release_if_owner().await
    }
}

#[async_trait::async_trait]
impl BlockReconciliationReadPort for ReconciliationAdapter {
    async fn leader_state(
        &self,
        next_height: BlockHeight,
    ) -> anyhow::Result<LeaderState> {
        match self {
            Self::Redis(adapter) => adapter.leader_state(next_height).await,
            Self::Noop(adapter) => adapter.leader_state(next_height).await,
        }
    }

    async fn release(&self) -> anyhow::Result<()> {
        match self {
            Self::Redis(adapter) => adapter.release().await,
            Self::Noop(adapter) => adapter.release().await,
        }
    }
}

impl Drop for RedisLeaderLeaseAdapter {
    fn drop(&mut self) {
        if std::sync::Arc::strong_count(&self.drop_release_guard) != 1 {
            return;
        }

        let redis_clients = self
            .redis_nodes
            .iter()
            .map(|redis_node| redis_node.redis_client.clone())
            .collect::<Vec<_>>();
        if let Ok(runtime_handle) = tokio::runtime::Handle::try_current() {
            let release_future = timeout(
                Duration::from_millis(100),
                Self::release_lease_on_clients(
                    redis_clients,
                    self.lease_key.clone(),
                    self.lease_owner_token.clone(),
                    self.node_timeout,
                ),
            );
            drop(runtime_handle.spawn(async move {
                if release_future.await.is_err() {
                    error!("Failed to release leader lease: timeout");
                }
            }));
            return;
        }

        Self::release_lease_on_clients_sync(
            redis_clients,
            self.lease_key.clone(),
            self.lease_owner_token.clone(),
        );
    }
}

impl BlockReconciliationWritePort for RedisLeaderLeaseAdapter {
    fn publish_produced_block(&self, block: &SealedBlock) -> anyhow::Result<()> {
        let epoch = match *self
            .current_epoch_token
            .lock()
            .map_err(|_| anyhow!("cannot access epoch token, poisoned lock"))?
        {
            Some(epoch) => epoch,
            None => {
                if matches!(
                    block.consensus,
                    fuel_core_types::blockchain::consensus::Consensus::Genesis(_)
                ) {
                    tracing::debug!(
                        "Skipping redis block publish for genesis block because fencing token is not initialized"
                    );
                    return Ok(());
                }
                return Err(anyhow!(
                    "Cannot publish block because fencing token is not initialized"
                ));
            }
        };
        let block_data = postcard::to_allocvec(block)?;
        let successes = self
            .publish_block_on_all_nodes(epoch, block, &block_data)
            .into_iter()
            .map(|result| match result {
                Ok(WriteBlockResult::Written) => true,
                Ok(_) => false,
                Err(err) => {
                    tracing::debug!("Redis publish on node failed: {err}");
                    false
                }
            })
            .filter(|success| *success)
            .count();
        if self.quorum_reached(successes) {
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to publish block to redis quorum with fencing checks"
            ))
        }
    }
}

#[async_trait::async_trait]
impl ConsensusModulePort for PoAAdapter {
    async fn manually_produce_blocks(
        &self,
        start_time: Option<Tai64>,
        number_of_blocks: u32,
    ) -> anyhow::Result<()> {
        self.manually_produce_blocks(start_time, Mode::Blocks { number_of_blocks })
            .await
    }
}

#[cfg(feature = "p2p")]
impl P2pPort for P2PAdapter {
    fn reserved_peers_count(&self) -> BoxStream<usize> {
        if let Some(service) = &self.service {
            Box::pin(
                BroadcastStream::new(service.subscribe_reserved_peers_count())
                    .filter_map(|result| result.ok()),
            )
        } else {
            Box::pin(tokio_stream::pending())
        }
    }
}

#[cfg(not(feature = "p2p"))]
impl P2pPort for P2PAdapter {
    fn reserved_peers_count(&self) -> BoxStream<usize> {
        Box::pin(tokio_stream::pending())
    }
}

pub struct InDirectoryPredefinedBlocks {
    path_to_directory: Option<PathBuf>,
}

impl InDirectoryPredefinedBlocks {
    pub fn new(path_to_directory: Option<PathBuf>) -> Self {
        Self { path_to_directory }
    }
}

impl PredefinedBlocks for InDirectoryPredefinedBlocks {
    fn get_block(&self, height: &BlockHeight) -> anyhow::Result<Option<Block>> {
        let Some(path) = &self.path_to_directory else {
            return Ok(None);
        };

        let block_height: u32 = (*height).into();
        if block_exists(path.as_path(), block_height) {
            let block_path = block_path(path.as_path(), block_height);
            let block_bytes = std::fs::read(block_path)?;
            let block: Block = serde_json::from_slice(block_bytes.as_slice())?;
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }
}

pub fn block_path(path_to_directory: &Path, block_height: u32) -> PathBuf {
    path_to_directory.join(format!("{block_height}.json"))
}

pub fn block_exists(path_to_directory: &Path, block_height: u32) -> bool {
    block_path(path_to_directory, block_height).exists()
}

impl TransactionPool for TxPoolAdapter {
    fn new_txs_watcher(&self) -> watch::Receiver<()> {
        self.service.get_new_executable_txs_notifier()
    }
}

#[async_trait::async_trait]
impl fuel_core_poa::ports::BlockProducer for BlockProducerAdapter {
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
        deadline: Instant,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        match source {
            TransactionsSource::TxPool => {
                self.block_producer
                    .produce_and_execute_block_txpool(height, block_time, deadline)
                    .await
            }
            TransactionsSource::SpecificTransactions(txs) => {
                self.block_producer
                    .produce_and_execute_block_transactions(height, block_time, txs)
                    .await
            }
        }
    }

    async fn produce_predefined_block(
        &self,
        block: &Block,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        self.block_producer
            .produce_and_execute_predefined(block, ())
            .await
    }
}

#[async_trait::async_trait]
impl BlockImporter for BlockImporterAdapter {
    async fn commit_result(
        &self,
        result: UncommittedImporterResult<Changes>,
    ) -> anyhow::Result<()> {
        self.block_importer
            .commit_result(result)
            .await
            .map_err(Into::into)
    }

    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()> {
        self.block_importer
            .execute_and_commit(block)
            .await
            .map_err(Into::into)
    }

    fn block_stream(&self) -> BoxStream<BlockImportInfo> {
        Box::pin(
            BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|result| result.ok())
                .map(|result| BlockImportInfo::from(result.shared_result)),
        )
    }

    fn latest_block_height(&self) -> anyhow::Result<Option<BlockHeight>> {
        self.database.latest_block_height().map_err(Into::into)
    }
}

#[cfg(all(test, feature = "leader_lock", not(feature = "not_leader_lock")))]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use fuel_core_importer::ports::BlockReconciliationWritePort;
    use fuel_core_poa::ports::BlockReconciliationReadPort;
    use fuel_core_types::blockchain::consensus::Consensus;
    use std::{
        io::Read as _,
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
        sync::mpsc,
        thread,
        time::{
            Duration,
            Instant,
        },
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_same_height_has_multiple_stream_entries_then_returns_highest_epoch_block()
     {
        // given
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:stream-conflict".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![redis.redis_url()],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");

        let low_epoch_block = poa_block_at_time(1, 10);
        let high_epoch_block = poa_block_at_time(1, 20);

        let low_epoch_data =
            postcard::to_allocvec(&low_epoch_block).expect("serialize block");
        let high_epoch_data =
            postcard::to_allocvec(&high_epoch_block).expect("serialize block");

        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &low_epoch_data, 1);
        append_stream_block(&mut conn, &stream_key, 1, &high_epoch_data, 2);

        // when
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then
        let unreconciled_blocks = match leader_state {
            LeaderState::UnreconciledBlocks(blocks) => blocks,
            other => panic!("Expected unreconciled blocks, got: {other:?}"),
        };
        assert_eq!(unreconciled_blocks.len(), 1);
        assert_eq!(
            unreconciled_blocks[0].entity.header().time(),
            high_epoch_block.entity.header().time(),
            "Expected reconciliation to pick the highest epoch block for the same height",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_same_height_same_epoch_has_multiple_stream_entries_then_keeps_latest_entry()
     {
        // given
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:equal-epoch-latest-entry".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![redis.redis_url()],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");

        let stale_block = poa_block_at_time(1, 10);
        let retry_block = poa_block_at_time(1, 20);
        let stale_data =
            postcard::to_allocvec(&stale_block).expect("stale block should serialize");
        let retry_data =
            postcard::to_allocvec(&retry_block).expect("retry block should serialize");

        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &stale_data, 1);
        append_stream_block(&mut conn, &stream_key, 1, &retry_data, 1);

        // when
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then
        let unreconciled_blocks = match leader_state {
            LeaderState::UnreconciledBlocks(blocks) => blocks,
            other => panic!("Expected unreconciled blocks, got: {other:?}"),
        };
        assert_eq!(unreconciled_blocks.len(), 1);
        assert_eq!(
            unreconciled_blocks[0].entity.id(),
            retry_block.entity.id(),
            "Expected reconciliation to keep latest stream entry for equal epoch",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_height_has_disagreeing_block_ids_then_repairs_with_highest_epoch_block()
     {
        // given: two different blocks at height 1 on different nodes, same epoch
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:epoch-quorum-block-mismatch".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = new_test_adapter(
            vec![
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            lease_key,
        );
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter should acquire lease"
        );

        let block_a = poa_block_at_time(1, 10);
        let block_b = poa_block_at_time(1, 20);
        let block_a_data =
            postcard::to_allocvec(&block_a).expect("block a should serialize");
        let block_b_data =
            postcard::to_allocvec(&block_b).expect("block b should serialize");

        let redis_a_client =
            redis::Client::open(redis_a.redis_url()).expect("redis a client should open");
        let redis_b_client =
            redis::Client::open(redis_b.redis_url()).expect("redis b client should open");
        let mut conn_a = redis_a_client
            .get_connection()
            .expect("redis a connection should open");
        let mut conn_b = redis_b_client
            .get_connection()
            .expect("redis b connection should open");

        // Both at epoch 7 but different block data — each on 1 node (sub-quorum)
        append_stream_block(&mut conn_a, &stream_key, 1, &block_a_data, 7);
        append_stream_block(&mut conn_b, &stream_key, 1, &block_b_data, 7);

        // when: leader reconciles — should pick one and repair to quorum
        // The repair writes to node C (empty), giving the winner 2/3
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then: one of the blocks is repaired and returned
        assert!(
            matches!(leader_state, LeaderState::UnreconciledBlocks(ref blocks) if blocks.len() == 1),
            "Expected repair to pick one block and reach quorum, got {leader_state:?}",
        );
    }

    /// Reproduces the devnet deadlock from April 17, 2026.
    ///
    /// The same block was written to all 3 nodes during re-promotion storms,
    /// so each node has the same block_id but with different epoch metadata.
    /// The old `(epoch, block_id)` vote grouping fragmented these into
    /// separate vote groups, with the max-epoch group having a count below
    /// quorum. Repair then failed because every node returned HEIGHT_EXISTS.
    ///
    /// With the fix (grouping by block_id only), all copies of the same
    /// block count toward quorum regardless of epoch metadata — so this
    /// state resolves without repair.
    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_same_block_has_different_epochs_across_nodes_then_reconciles_without_repair()
     {
        // given: same block on all 3 nodes, but with different epochs
        // (as happens when re-promotion writes race during production)
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:same-block-different-epochs".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = new_test_adapter(
            vec![
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            lease_key,
        );
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter should acquire lease"
        );

        // Same block (same data, same block_id) on all 3 nodes, but each
        // with a different epoch. This simulates what happens when the
        // original leader was re-promoted repeatedly during a race,
        // writing the same block content each time with a bumped epoch.
        let block = poa_block_at_time(1, 10);
        let block_data = postcard::to_allocvec(&block).expect("should serialize");

        let redis_a_client =
            redis::Client::open(redis_a.redis_url()).expect("redis a client");
        let redis_b_client =
            redis::Client::open(redis_b.redis_url()).expect("redis b client");
        let redis_c_client =
            redis::Client::open(redis_c.redis_url()).expect("redis c client");
        let mut conn_a = redis_a_client.get_connection().expect("redis a conn");
        let mut conn_b = redis_b_client.get_connection().expect("redis b conn");
        let mut conn_c = redis_c_client.get_connection().expect("redis c conn");

        // Same block_id, three different epochs
        append_stream_block(&mut conn_a, &stream_key, 1, &block_data, 5);
        append_stream_block(&mut conn_b, &stream_key, 1, &block_data, 7);
        append_stream_block(&mut conn_c, &stream_key, 1, &block_data, 9);

        // when: leader reconciles
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then: the block is reconciled directly (no repair needed). Without
        // the fix, the old logic would have split the 3 copies into 3 vote
        // groups and tried to repair the max-epoch group (count=1), which
        // would deadlock because every node returns HEIGHT_EXISTS.
        assert!(
            matches!(leader_state, LeaderState::UnreconciledBlocks(ref blocks) if blocks.len() == 1),
            "Expected block to be reconciled from quorum across mixed epochs, got {leader_state:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_same_height_entry_exists_on_less_than_quorum_nodes_then_repairs_it()
     {
        // given: orphan block on only 1 of 3 nodes (below quorum)
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:below-quorum".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = new_test_adapter(
            vec![
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            lease_key,
        );
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter should acquire lease"
        );

        let orphan_block = poa_block_at_time(1, 10);
        let orphan_block_data =
            postcard::to_allocvec(&orphan_block).expect("orphan block should serialize");

        let redis_client =
            redis::Client::open(redis_a.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &orphan_block_data, 1);

        // when: leader reconciles — should repair the orphan to quorum
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then: orphan was reproposed to other nodes and returned for import
        assert!(
            matches!(leader_state, LeaderState::UnreconciledBlocks(ref blocks) if blocks.len() == 1),
            "Expected sub-quorum entry to be repaired and returned, got {leader_state:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_contiguous_heights_have_quorum_then_repairs_sub_quorum_tail()
     {
        // given: h1, h2 on quorum (a,b). h3 below quorum (a only).
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:contiguous-quorum".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = new_test_adapter(
            vec![
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            lease_key,
        );
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter should acquire lease"
        );

        let h1 = poa_block_at_time(1, 10);
        let h2 = poa_block_at_time(2, 20);
        let h3 = poa_block_at_time(3, 30);
        let h1_data = postcard::to_allocvec(&h1).expect("h1 should serialize");
        let h2_data = postcard::to_allocvec(&h2).expect("h2 should serialize");
        let h3_data = postcard::to_allocvec(&h3).expect("h3 should serialize");

        let redis_a_client =
            redis::Client::open(redis_a.redis_url()).expect("redis a client should open");
        let redis_b_client =
            redis::Client::open(redis_b.redis_url()).expect("redis b client should open");
        let mut conn_a = redis_a_client
            .get_connection()
            .expect("redis a connection should open");
        let mut conn_b = redis_b_client
            .get_connection()
            .expect("redis b connection should open");

        // h1 on quorum (a,b)
        append_stream_block(&mut conn_a, &stream_key, 1, &h1_data, 1);
        append_stream_block(&mut conn_b, &stream_key, 1, &h1_data, 1);
        // h2 on quorum (a,b)
        append_stream_block(&mut conn_a, &stream_key, 2, &h2_data, 1);
        append_stream_block(&mut conn_b, &stream_key, 2, &h2_data, 1);
        // h3 below quorum (a only)
        append_stream_block(&mut conn_a, &stream_key, 3, &h3_data, 1);

        // when: leader reconciles — h3 should be repaired to quorum
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then: all 3 heights returned (h3 was repaired)
        let unreconciled_blocks = match leader_state {
            LeaderState::UnreconciledBlocks(blocks) => blocks,
            other => panic!("Expected unreconciled blocks, got: {other:?}"),
        };
        assert_eq!(
            unreconciled_blocks
                .iter()
                .map(|b| u32::from(*b.entity.header().height()))
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "Expected all heights including repaired sub-quorum h3",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_contiguous_quorum_blocks_are_present_then_returns_all_available_contiguous_blocks()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:contiguous-over-128".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");
        let redis_a_client =
            redis::Client::open(redis_a.redis_url()).expect("redis a client should open");
        let redis_b_client =
            redis::Client::open(redis_b.redis_url()).expect("redis b client should open");
        let redis_c_client =
            redis::Client::open(redis_c.redis_url()).expect("redis c client should open");
        let mut conn_a = redis_a_client
            .get_connection()
            .expect("redis a connection should open");
        let mut conn_b = redis_b_client
            .get_connection()
            .expect("redis b connection should open");
        let mut conn_c = redis_c_client
            .get_connection()
            .expect("redis c connection should open");
        let _ = &mut conn_c;

        (1_u32..=129_u32).for_each(|height| {
            let block = poa_block_at_time(height, u64::from(height));
            let block_data =
                postcard::to_allocvec(&block).expect("block should serialize");
            append_stream_block(&mut conn_a, &stream_key, height, &block_data, 1);
            append_stream_block(&mut conn_b, &stream_key, height, &block_data, 1);
        });

        // when
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then
        let unreconciled_blocks = match leader_state {
            LeaderState::UnreconciledBlocks(blocks) => blocks,
            other => panic!("Expected unreconciled blocks, got: {other:?}"),
        };
        assert_eq!(
            unreconciled_blocks.len(),
            129,
            "Expected all contiguous quorum-backed heights to reconcile in one call",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__when_fencing_token_is_uninitialized_then_returns_error()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:missing-epoch".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = new_test_adapter(redis_urls, lease_key);
        let block = poa_block_at_time(1, 100);

        // when
        let publish_result = adapter.publish_produced_block(&block);

        // then
        assert!(
            publish_result.is_err(),
            "Publish should fail when fencing token is not initialized"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn release__when_adapter_is_not_lease_owner_then_returns_ok() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:release-follower".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = new_test_adapter(redis_urls, lease_key);

        // when
        let release_result = adapter.release().await;

        // then
        assert!(
            release_result.is_ok(),
            "Release should be idempotent for adapters that do not own quorum lease"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drop__when_non_last_clone_is_dropped_then_does_not_release_shared_lease() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:drop-non-last-clone".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Adapter should acquire lease"
        );
        let adapter_clone = adapter.clone();
        let owner_token = adapter.lease_owner_token.clone();

        // when
        drop(adapter_clone);
        sleep(Duration::from_millis(50)).await;

        // then
        let owners = redis_urls
            .iter()
            .filter(|redis_url| {
                read_lease_owner(redis_url, &lease_key).as_deref()
                    == Some(owner_token.as_str())
            })
            .count();
        assert!(
            owners >= 2,
            "Dropping a non-last clone must not release quorum lease ownership"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_lease_is_free_then_acquires_quorum_ownership() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:acquire-on-leader-state".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = RedisLeaderLeaseAdapter::new(
            redis_urls.clone(),
            lease_key.clone(),
            Duration::from_millis(500),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");

        // when
        let state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");
        let owners = redis_urls
            .iter()
            .filter(|redis_url| {
                read_lease_owner(redis_url, &lease_key).as_deref()
                    == Some(adapter.lease_owner_token.as_str())
            })
            .count();

        // then
        assert!(
            matches!(state, LeaderState::ReconciledLeader),
            "leader_state should acquire and report leader ownership when lease is free"
        );
        assert!(
            owners >= 2,
            "Lease ownership should be present on quorum after acquisition"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_lease_expires_then_another_adapter_becomes_leader() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:ttl-expiry-handoff".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let first_adapter = RedisLeaderLeaseAdapter::new(
            redis_urls.clone(),
            lease_key.clone(),
            Duration::from_millis(300),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("first adapter should be created");
        let second_adapter = RedisLeaderLeaseAdapter::new(
            redis_urls.clone(),
            lease_key.clone(),
            Duration::from_millis(300),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("second adapter should be created");

        let first_state = first_adapter
            .leader_state(1.into())
            .await
            .expect("first leader_state should succeed");
        sleep(Duration::from_millis(900)).await;

        // when
        let second_state = second_adapter
            .leader_state(1.into())
            .await
            .expect("second leader_state should succeed");
        let second_owner_count = redis_urls
            .iter()
            .filter(|redis_url| {
                read_lease_owner(redis_url, &lease_key).as_deref()
                    == Some(second_adapter.lease_owner_token.as_str())
            })
            .count();

        // then
        assert!(
            matches!(first_state, LeaderState::ReconciledLeader),
            "First adapter should acquire lease initially"
        );
        assert!(
            matches!(second_state, LeaderState::ReconciledLeader),
            "Second adapter should become leader after TTL expiry"
        );
        assert!(
            second_owner_count >= 2,
            "Second adapter should own lease on quorum nodes after takeover"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__when_previous_leader_writes_after_handoff_then_rejects_zombie_write()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:zombie-leader".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let old_leader = new_test_adapter(redis_urls.clone(), lease_key.clone());
        let current_leader = new_test_adapter(redis_urls, lease_key.clone());
        let block = poa_block_at_time(1, 111);

        assert!(
            old_leader
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Old leader should acquire initial lease"
        );
        clear_lease_on_nodes(
            &[
                redis_a.redis_url(),
                redis_b.redis_url(),
                redis_c.redis_url(),
            ],
            &lease_key,
        );
        assert!(
            current_leader
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Current leader should acquire lease after handoff"
        );

        // when
        let zombie_write = old_leader.publish_produced_block(&block);

        // then
        assert!(
            zombie_write.is_err(),
            "Old leader write should be fenced after handoff"
        );
        let current_state = current_leader
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");
        assert!(
            matches!(current_state, LeaderState::ReconciledLeader),
            "Zombie partial writes must not be considered committed"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__when_epoch_is_behind_on_one_node_then_first_write_heals_epoch()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:epoch-healing".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = new_test_adapter(redis_urls, lease_key.clone());
        let epoch_key = format!("{lease_key}:epoch:token");
        let block = poa_block_at_time(1, 222);
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Adapter should acquire lease"
        );
        let leader_epoch = (*adapter.current_epoch_token.lock().expect("poisoned lock"))
            .expect("epoch should be initialized");
        let stale_epoch = leader_epoch.saturating_sub(1);
        set_epoch(&redis_a.redis_url(), &epoch_key, stale_epoch);

        // when
        let publish_result = adapter.publish_produced_block(&block);

        // then
        assert!(
            publish_result.is_ok(),
            "Publish should still succeed on quorum"
        );
        let healed_epoch = read_epoch(&redis_a.redis_url(), &epoch_key);
        assert_eq!(
            healed_epoch, leader_epoch,
            "First successful write should heal lagging epoch"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__when_write_succeeds_then_extends_lease_ttl() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:publish-extends-lease-ttl".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = RedisLeaderLeaseAdapter::new(
            redis_urls.clone(),
            lease_key.clone(),
            Duration::from_millis(700),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");
        let block = poa_block_at_time(1, 444);
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Adapter should acquire lease"
        );
        sleep(Duration::from_millis(500)).await;

        // when
        let publish_result = adapter.publish_produced_block(&block);
        sleep(Duration::from_millis(400)).await;
        let owners = redis_urls
            .iter()
            .filter(|redis_url| {
                read_lease_owner(redis_url, &lease_key).as_deref()
                    == Some(adapter.lease_owner_token.as_str())
            })
            .count();

        // then
        assert!(publish_result.is_ok(), "Publish should succeed on quorum");
        assert!(
            owners >= 2,
            "Successful write should extend lease TTL on quorum beyond original window"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__when_write_succeeds_on_less_than_quorum_then_entry_is_not_reconciled()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:partial-write".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];
        let adapter = new_test_adapter(redis_urls, lease_key.clone());
        let block = poa_block_at_time(1, 333);
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Adapter should acquire lease"
        );
        set_lease_owner(
            &redis_b.redis_url(),
            &lease_key,
            "other-owner",
            adapter.lease_ttl_millis,
        );
        set_lease_owner(
            &redis_c.redis_url(),
            &lease_key,
            "other-owner",
            adapter.lease_ttl_millis,
        );

        // when
        let publish_result = adapter.publish_produced_block(&block);
        let unreconciled = adapter.unreconciled_blocks(1.into()).await;

        // then
        assert!(
            publish_result.is_err(),
            "Publish must fail when fewer than quorum nodes accept write"
        );
        assert!(
            unreconciled.is_err(),
            "Unresolved backlog should return an error instead of empty result"
        );
        assert_eq!(
            stream_len(&redis_a.redis_url(), &stream_key),
            1,
            "One orphan entry should exist on the single successful node"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unreconciled_blocks__when_quorum_latest_height_is_below_next_height_then_returns_empty()
     {
        // given
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:cursor-fast-path".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![redis.redis_url()],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");
        let block = poa_block_at_time(1, 10);
        let block_data = postcard::to_allocvec(&block).expect("serialize block");
        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &block_data, 1);

        // when
        let blocks = adapter
            .unreconciled_blocks(2.into())
            .await
            .expect("reconciliation read should succeed");

        // then
        assert!(
            blocks.is_empty(),
            "Expected fast path to skip full reconciliation"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn read_stream_entries_on_node__when_next_height_is_provided_then_reads_matching_entries()
     {
        // given
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:cursor-incremental-read".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![redis.redis_url()],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");
        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        let h1 = poa_block_at_time(1, 10);
        let h2 = poa_block_at_time(2, 20);
        let h3 = poa_block_at_time(3, 30);
        let h1_data = postcard::to_allocvec(&h1).expect("serialize block");
        let h2_data = postcard::to_allocvec(&h2).expect("serialize block");
        let h3_data = postcard::to_allocvec(&h3).expect("serialize block");
        append_stream_block(&mut conn, &stream_key, 1, &h1_data, 1);
        append_stream_block(&mut conn, &stream_key, 2, &h2_data, 1);
        let redis_node = adapter.redis_nodes[0].clone();

        // when
        let first_read = adapter
            .read_stream_entries_on_node(&redis_node, 1, 1000)
            .await
            .expect("first read should succeed");
        append_stream_block(&mut conn, &stream_key, 3, &h3_data, 1);
        let second_read = adapter
            .read_stream_entries_on_node(&redis_node, 3, 1000)
            .await
            .expect("second read should succeed");

        // then
        assert_eq!(
            first_read.len(),
            2,
            "Expected initial read to include existing entries"
        );
        assert_eq!(
            second_read.len(),
            1,
            "Expected height-filtered read to include only matching entries"
        );
        assert_eq!(
            u32::from(*second_read[0].2.entity.header().height()),
            3,
            "Expected only the requested next height"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn read_stream_entries_on_node__when_max_entries_is_small_then_caps_results() {
        // given
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:cursor-pagination".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let adapter = RedisLeaderLeaseAdapter::new(
            vec![redis.redis_url()],
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created");
        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        let h1 = poa_block_at_time(1, 10);
        let h2 = poa_block_at_time(2, 20);
        let h3 = poa_block_at_time(3, 30);
        let h1_data = postcard::to_allocvec(&h1).expect("serialize block");
        let h2_data = postcard::to_allocvec(&h2).expect("serialize block");
        let h3_data = postcard::to_allocvec(&h3).expect("serialize block");
        append_stream_block(&mut conn, &stream_key, 1, &h1_data, 1);
        append_stream_block(&mut conn, &stream_key, 2, &h2_data, 1);
        append_stream_block(&mut conn, &stream_key, 3, &h3_data, 1);
        let redis_node = adapter.redis_nodes[0].clone();

        // when
        let first_page = adapter
            .read_stream_entries_on_node(&redis_node, 1, 2)
            .await
            .expect("first page should succeed");
        let second_page = adapter
            .read_stream_entries_on_node(&redis_node, 3, 2)
            .await
            .expect("second page should succeed");

        // then
        assert_eq!(first_page.len(), 2, "Expected first page to be capped");
        assert_eq!(
            u32::from(*first_page[0].2.entity.header().height()),
            1,
            "Expected first page to start from earliest height"
        );
        assert_eq!(
            u32::from(*first_page[1].2.entity.header().height()),
            2,
            "Expected first page to include second height"
        );
        assert_eq!(
            second_page.len(),
            1,
            "Expected height filter to return only matching trailing entry"
        );
        assert_eq!(
            u32::from(*second_page[0].2.entity.header().height()),
            3,
            "Expected second read to include only the requested next height"
        );
    }

    /// When a partial publish leaves a stale entry at a given height,
    /// a subsequent write at the same height is rejected by
    /// write_block.lua's HEIGHT_EXISTS check. This prevents two blocks
    /// at the same height from coexisting in the stream, which would
    /// cause a fork if a different leader also achieved quorum at that
    /// height.
    #[tokio::test(flavor = "multi_thread")]
    async fn partial_publish_then_retry_at_same_height__new_leader_reconciles_stale_block()
     {
        let redis = RedisTestServer::spawn();
        let lease_key = "poa:test:fork-repro".to_string();
        let stream_key = format!("{lease_key}:block:stream");

        let adapter_a = new_test_adapter(vec![redis.redis_url()], lease_key.clone());
        assert!(
            adapter_a
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter_a should acquire lease"
        );
        let epoch_a = (*adapter_a.current_epoch_token.lock().expect("lock"))
            .expect("epoch should be set");

        // Simulate stale partial publish by writing directly to stream
        let block_a = poa_block_at_time(1, 100);
        let block_a_data = postcard::to_allocvec(&block_a).expect("serialize block_a");
        let redis_client =
            redis::Client::open(redis.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &block_a_data, epoch_a as u32);

        // Leader A retries with a different block at the same height —
        // this should FAIL because height 1 already exists in the stream.
        let block_b = poa_block_at_time(1, 999);
        assert_ne!(
            block_a.entity.header().time(),
            block_b.entity.header().time()
        );

        let result = adapter_a.publish_produced_block(&block_b);
        assert!(
            result.is_err(),
            "publish at same height should fail due to HEIGHT_EXISTS"
        );

        // Stream should still have only the original entry
        assert_eq!(stream_len(&redis.redis_url(), &stream_key), 1);

        adapter_a.release().await.expect("release should succeed");

        // A new leader reconciles and sees block_a (the only entry)
        let adapter_b = new_test_adapter(vec![redis.redis_url()], lease_key.clone());
        assert!(
            adapter_b
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter_b should acquire lease"
        );

        let unreconciled = adapter_b
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation should succeed");

        assert_eq!(unreconciled.len(), 1, "Should reconcile exactly one block");
        assert_eq!(
            unreconciled[0].entity.header().time(),
            block_a.entity.header().time(),
            "Reconciliation should return block_a (the only entry in the stream)"
        );
    }

    // ---------------------------------------------------------------------
    // Regression tests for the 2026-04-22 mainnet hang.
    //
    // On v0.47.4 the deployed binary linked `redis 0.27.x`. That release of
    // `redis::Client::get_connection_with_timeout` only applied the timeout
    // to the TCP connect step; the post-connect handshake pipeline
    // (`CLIENT SETINFO LIB-NAME`, `CLIENT SETINFO LIB-VER`) ran without any
    // socket-level read/write timeout. A peer that accepted TCP but stopped
    // responding caused the call to hang indefinitely. Combined with the
    // previous `std::thread::scope`-based `publish_block_on_all_nodes`, one
    // stuck per-node thread wedged every block publish forever and halted
    // block production on mainnet for ~22 minutes.
    //
    // The fix in this PR has two parts:
    //   1. Bump `redis` to 1.2. Upstream's `connect()` now sets read/write
    //      timeouts on the socket BEFORE running the handshake pipeline and
    //      clears them afterward.
    //   2. Replace `std::thread::scope` in `publish_block_on_all_nodes`
    //      with detached `std::thread::spawn` workers reporting into an
    //      mpsc channel, returning as soon as `Written` quorum is reached.
    //      Stragglers are abandoned.
    //
    // The first test below targets (1) directly: raw library call against a
    // half-alive peer must now be bounded.
    //
    // The second test targets (2) at the adapter level: with 3 healthy
    // redis-server processes + 1 half-alive TCP listener (4 nodes, quorum =
    // 3), `publish_produced_block` must complete within a tight wall-clock
    // deadline. On pre-fix code this call hangs forever because the scoped
    // thread for the half-alive node never exits; on the fixed code the
    // publish returns as soon as the 3 healthy nodes acknowledge.
    // ---------------------------------------------------------------------

    /// Accepts TCP connections and drains incoming bytes into a buffer
    /// without ever writing a single byte back. Emulates an ElastiCache /
    /// Valkey node in the "half-alive" recovery state observed on 2026-04-22.
    fn spawn_halflive_redis_listener() -> u16 {
        let listener =
            TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
                .expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else {
                    continue;
                };
                thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = stream.read(&mut buf) {
                        if n == 0 {
                            return;
                        }
                    }
                });
            }
        });
        port
    }

    /// Library-level regression guard for the upstream `redis 0.27.x` bug.
    /// On that version this test would hang indefinitely and the process
    /// would need to be killed. On `redis >= 1.2` the call returns an `Err`
    /// within roughly `2 * node_timeout` (one timeout per handshake
    /// pipeline command: `CLIENT SETINFO LIB-NAME`, `CLIENT SETINFO LIB-VER`).
    #[test]
    fn redis_get_connection_with_timeout__is_bounded_against_halflive_peer() {
        const NODE_TIMEOUT: Duration = Duration::from_secs(1);
        const MAX_WAIT: Duration = Duration::from_secs(5);

        let port = spawn_halflive_redis_listener();
        let url = format!("redis://127.0.0.1:{port}/");
        let client = redis::Client::open(url).expect("client open");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let start = Instant::now();
            let result = client.get_connection_with_timeout(NODE_TIMEOUT);
            let _ = tx.send((start.elapsed(), result.is_ok()));
        });

        match rx.recv_timeout(MAX_WAIT) {
            Ok((elapsed, ok)) => {
                assert!(
                    !ok,
                    "expected err against a half-alive peer, got Ok after \
                     {elapsed:?}",
                );
                assert!(
                    elapsed <= MAX_WAIT,
                    "call took {elapsed:?}, expected to be bounded under \
                     {MAX_WAIT:?}",
                );
            }
            Err(_) => {
                panic!(
                    "REGRESSION: get_connection_with_timeout did not \
                     return within {MAX_WAIT:?} against a half-alive \
                     peer. The redis-crate upstream bug from 0.27.x has \
                     reappeared.",
                );
            }
        }
    }

    /// Adapter-level regression test for the block-production hang. With 3
    /// healthy redis nodes and 1 half-alive TCP listener (4 nodes total,
    /// quorum = 3), `publish_produced_block` must complete within a
    /// wall-clock deadline much smaller than "forever".
    ///
    /// On the pre-fix code (`std::thread::scope` waiting for all 4 handles
    /// to join) this call hangs until the process is killed. On the fixed
    /// code the detached threads report via an mpsc channel and
    /// `publish_block_on_all_nodes` returns as soon as the 3 healthy nodes
    /// acknowledge `Written`.
    #[tokio::test(flavor = "multi_thread")]
    async fn publish_produced_block__returns_within_bound_when_one_node_is_half_alive() {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let halflive_port = spawn_halflive_redis_listener();
        let lease_key = "poa:test:halflive-publish".to_string();
        let urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
            format!("redis://127.0.0.1:{halflive_port}/"),
        ];
        let adapter = new_test_adapter(urls, lease_key);
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "should acquire quorum from 3 healthy nodes out of 4",
        );

        // when — publish is sync; offload to a blocking thread so we can
        // enforce a wall-clock deadline via tokio::time::timeout. Without
        // the fix this task never completes.
        let block = poa_block_at_time(1, 111);
        let publish_adapter = adapter.clone();
        let start = Instant::now();
        let publish = tokio::task::spawn_blocking(move || {
            publish_adapter.publish_produced_block(&block)
        });
        let deadline = Duration::from_secs(5);
        let result = tokio::time::timeout(deadline, publish)
            .await
            .expect(
                "publish_produced_block must return within the deadline — \
                 on the pre-fix code it hangs forever against a half-alive \
                 peer",
            )
            .expect("spawn_blocking should not panic");
        let elapsed = start.elapsed();

        // then
        result.expect("publish should succeed: 3/4 healthy nodes is quorum");
        assert!(
            elapsed < deadline,
            "publish took {elapsed:?}, expected well under {deadline:?}",
        );
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
        let socket =
            TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
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
            .expect("Failed to spawn redis-server")
    }

    fn wait_for_redis_ready(port: u16) {
        let addr = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, port);
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(5);
        while start.elapsed() < timeout {
            if TcpStream::connect(addr).is_ok() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("Redis server did not become ready on port {port}");
    }

    fn poa_block_at_time(height: u32, timestamp: u64) -> SealedBlock {
        let mut block = Block::default();
        block.header_mut().set_block_height(height.into());
        block
            .header_mut()
            .set_time(fuel_core_types::tai64::Tai64(timestamp));
        block.header_mut().recalculate_metadata();
        SealedBlock {
            entity: block,
            consensus: Consensus::PoA(Default::default()),
        }
    }

    fn append_stream_block(
        conn: &mut redis::Connection,
        stream_key: &str,
        height: u32,
        data: &[u8],
        epoch: u32,
    ) {
        let _: String = redis::cmd("XADD")
            .arg(stream_key)
            .arg("*")
            .arg("height")
            .arg(height)
            .arg("data")
            .arg(data)
            .arg("epoch")
            .arg(epoch)
            .arg("timestamp")
            .arg(epoch)
            .query(conn)
            .expect("stream write should succeed");
    }

    fn new_test_adapter(
        redis_urls: Vec<String>,
        lease_key: String,
    ) -> RedisLeaderLeaseAdapter {
        RedisLeaderLeaseAdapter::new(
            redis_urls,
            lease_key,
            Duration::from_secs(2),
            Duration::from_millis(100),
            Duration::from_millis(50),
            Duration::from_millis(0),
            1,
            1000,
        )
        .expect("adapter should be created")
    }

    fn set_epoch(redis_url: &str, epoch_key: &str, epoch: u64) {
        let redis_client =
            redis::Client::open(redis_url).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        let _: () = redis::cmd("SET")
            .arg(epoch_key)
            .arg(epoch)
            .query(&mut conn)
            .expect("epoch set should succeed");
    }

    fn read_epoch(redis_url: &str, epoch_key: &str) -> u64 {
        let redis_client =
            redis::Client::open(redis_url).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        let epoch: Option<u64> = redis::cmd("GET")
            .arg(epoch_key)
            .query(&mut conn)
            .expect("epoch get should succeed");
        epoch.expect("epoch should exist")
    }

    fn set_lease_owner(
        redis_url: &str,
        lease_key: &str,
        owner: &str,
        lease_ttl_millis: u64,
    ) {
        let redis_client =
            redis::Client::open(redis_url).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        let _: () = redis::cmd("SET")
            .arg(lease_key)
            .arg(owner)
            .arg("PX")
            .arg(lease_ttl_millis)
            .query(&mut conn)
            .expect("lease owner set should succeed");
    }

    fn read_lease_owner(redis_url: &str, lease_key: &str) -> Option<String> {
        let redis_client =
            redis::Client::open(redis_url).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        redis::cmd("GET")
            .arg(lease_key)
            .query(&mut conn)
            .expect("lease owner get should succeed")
    }

    fn clear_lease_on_nodes(redis_urls: &[String], lease_key: &str) {
        redis_urls.iter().for_each(|redis_url| {
            let redis_client = redis::Client::open(redis_url.as_str())
                .expect("redis client should open");
            let mut conn = redis_client
                .get_connection()
                .expect("redis connection should open");
            let _: () = redis::cmd("DEL")
                .arg(lease_key)
                .query(&mut conn)
                .expect("lease delete should succeed");
        });
    }

    fn stream_len(redis_url: &str, stream_key: &str) -> usize {
        let redis_client =
            redis::Client::open(redis_url).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        redis::cmd("XLEN")
            .arg(stream_key)
            .query(&mut conn)
            .expect("stream length query should succeed")
    }

    /// When Redis read calls fail on a quorum of nodes,
    /// `unreconciled_blocks` must return an error — not silently
    /// return an empty list that would let the caller produce
    /// a divergent block.
    #[tokio::test(flavor = "multi_thread")]
    async fn unreconciled_blocks__when_reads_fail_on_quorum_nodes__returns_error() {
        // given — 3 Redis nodes, leader A publishes block to all 3
        let mut redis_a = RedisTestServer::spawn();
        let mut redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:read-failure-fork".to_string();
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        let adapter_a = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter_a
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Leader A should acquire lease"
        );

        let block = poa_block_at_time(1, 100);
        adapter_a
            .publish_produced_block(&block)
            .expect("publish should succeed on all 3 nodes");

        // Verify block exists on all 3 nodes
        let stream_key = format!("{lease_key}:block:stream");
        assert_eq!(stream_len(&redis_a.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_b.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_c.redis_url(), &stream_key), 1);

        // Simulate A releasing lease
        adapter_a.release().await.expect("release should succeed");

        // when — kill 2 of 3 Redis nodes BEFORE new leader reconciles
        redis_a.stop();
        redis_b.stop();

        let adapter_b = new_test_adapter(redis_urls.clone(), lease_key.clone());
        // Manually set epoch so we can call unreconciled_blocks directly
        {
            let mut epoch = adapter_b.current_epoch_token.lock().expect("lock");
            *epoch = Some(99);
        }

        let result = adapter_b.unreconciled_blocks(1.into()).await;

        // then — must return Err, not Ok([])
        assert!(
            result.is_err(),
            "unreconciled_blocks must return error when reads fail on quorum of nodes"
        );
    }

    /// Proves that when a Redis node restarts (losing all in-memory data),
    /// a block that was published to exactly quorum nodes drops below quorum
    /// and reconciliation cannot find it — enabling a fork.
    #[tokio::test(flavor = "multi_thread")]
    async fn unreconciled_blocks__when_redis_node_restarts_and_loses_data__drops_block_below_quorum()
     {
        // given — 3 Redis nodes, leader A publishes block to nodes A and B only
        // (simulating a partial publish where node C timed out)
        let redis_a = RedisTestServer::spawn();
        let mut redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:data-loss-fork".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        let adapter_a = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter_a
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Leader A should acquire lease"
        );
        let epoch_a = (*adapter_a.current_epoch_token.lock().expect("lock"))
            .expect("epoch should be set");

        // Publish block to nodes A and B only (simulating node C timeout).
        // We write directly to simulate the partial publish that still
        // reaches quorum (2/3).
        let block = poa_block_at_time(1, 100);
        let block_data = postcard::to_allocvec(&block).expect("serialize");

        let client_a = redis::Client::open(redis_a.redis_url()).expect("client");
        let mut conn_a = client_a.get_connection().expect("conn");
        let client_b = redis::Client::open(redis_b.redis_url()).expect("client");
        let mut conn_b = client_b.get_connection().expect("conn");

        append_stream_block(&mut conn_a, &stream_key, 1, &block_data, epoch_a as u32);
        append_stream_block(&mut conn_b, &stream_key, 1, &block_data, epoch_a as u32);
        // Node C has no entry (simulated timeout during publish)

        // Verify: block on A and B, not on C
        assert_eq!(stream_len(&redis_a.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_b.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_c.redis_url(), &stream_key), 0);

        // Confirm reconciliation works BEFORE data loss — quorum=2, both A and B have it
        let pre_loss = adapter_a
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation should succeed");
        assert_eq!(
            pre_loss.len(),
            1,
            "Block should be reconcilable with 2/3 nodes having it"
        );

        // when — Redis node B restarts (pod eviction / rolling deploy / AMI drift)
        // All in-memory data is lost (no persistence configured)
        drop(conn_b);
        drop(client_b);
        redis_b.stop();
        redis_b.start();

        // Verify node B lost its stream data
        assert_eq!(
            stream_len(&redis_b.redis_url(), &stream_key),
            0,
            "Restarted node should have empty stream"
        );

        // Release A's lease so B can acquire
        adapter_a.release().await.expect("release should succeed");

        // New leader acquires
        let adapter_b = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter_b
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "New leader should acquire lease"
        );

        let post_loss = adapter_b
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation should succeed");

        // then — repair reproposed the block from node A to node B (now empty)
        // and node C, reaching quorum again. The block is recovered.
        assert_eq!(
            post_loss.len(),
            1,
            "Repair should recover the block by reproposing from node A to the other nodes"
        );
    }

    /// After an election storm where leader A wins on nodes 1,2 but
    /// another candidate held node 3, `has_lease_owner_quorum` should
    /// expand the lock to node 3 once it's free. Subsequent block
    /// writes then go to all 3 nodes instead of just 2.
    #[tokio::test(flavor = "multi_thread")]
    async fn has_lease_owner_quorum__expands_lock_to_non_owned_nodes() {
        // given — 3 Redis nodes
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:lock-expansion".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        // Simulate election storm: candidate B grabs node C first
        let candidate_b = new_test_adapter(redis_urls.clone(), lease_key.clone());
        // Manually acquire on node C only (simulate B winning SET NX on C)
        {
            let client = redis::Client::open(redis_c.redis_url()).expect("client");
            let mut conn = client.get_connection().expect("conn");
            let _: () = redis::cmd("SET")
                .arg(&lease_key)
                .arg(&candidate_b.lease_owner_token)
                .arg("PX")
                .arg(5000u64)
                .query(&mut conn)
                .expect("set should succeed");
        }

        // Leader A acquires — gets nodes A,B but not C (B holds it)
        let adapter_a = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter_a
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Leader A should acquire quorum (2/3)"
        );

        // Verify A owns nodes A,B but NOT node C
        let owns_a = read_lease_owner(&redis_a.redis_url(), &lease_key)
            == Some(adapter_a.lease_owner_token.clone());
        let owns_b = read_lease_owner(&redis_b.redis_url(), &lease_key)
            == Some(adapter_a.lease_owner_token.clone());
        let owns_c = read_lease_owner(&redis_c.redis_url(), &lease_key)
            == Some(adapter_a.lease_owner_token.clone());
        assert!(owns_a && owns_b, "A should own nodes A and B");
        assert!(!owns_c, "A should NOT own node C (held by B)");

        // Candidate B releases node C (simulating failed-quorum cleanup)
        clear_lease_on_nodes(&[redis_c.redis_url()], &lease_key);
        assert!(
            read_lease_owner(&redis_c.redis_url(), &lease_key).is_none(),
            "Node C should be free after B releases"
        );

        // when — A calls has_lease_owner_quorum (which now expands)
        let has_quorum = adapter_a
            .has_lease_owner_quorum()
            .await
            .expect("quorum check should succeed");
        assert!(has_quorum, "A should still have quorum");

        // then — A should now own node C too
        let owns_c_after = read_lease_owner(&redis_c.redis_url(), &lease_key)
            == Some(adapter_a.lease_owner_token.clone());
        assert!(owns_c_after, "Lock expansion should have acquired node C");

        // Verify writes now go to all 3 nodes
        let block = poa_block_at_time(1, 100);
        adapter_a
            .publish_produced_block(&block)
            .expect("publish should succeed");

        assert_eq!(stream_len(&redis_a.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_b.redis_url(), &stream_key), 1);
        assert_eq!(
            stream_len(&redis_c.redis_url(), &stream_key),
            1,
            "Block should be written to expanded node C"
        );
    }

    /// When lock expansion acquires a node with a higher epoch
    /// (from election storm drift), the leader adopts the higher epoch
    /// so write_block.lua succeeds on all nodes.
    #[tokio::test(flavor = "multi_thread")]
    async fn has_lease_owner_quorum__adopts_higher_epoch_from_expanded_node() {
        // given — 3 Redis nodes
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:epoch-adoption".to_string();
        let epoch_key = format!("{lease_key}:epoch:token");
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        // Simulate election storm: B promotes on node C (incrementing epoch)
        // then fails quorum and releases the lock, leaving epoch drifted
        let candidate_b = new_test_adapter(redis_urls.clone(), lease_key.clone());
        {
            let client = redis::Client::open(redis_c.redis_url()).expect("client");
            let mut conn = client.get_connection().expect("conn");
            // Simulate B's promote_leader.lua on node C: SET NX + INCR
            let _: () = redis::cmd("SET")
                .arg(&lease_key)
                .arg(&candidate_b.lease_owner_token)
                .arg("PX")
                .arg(5000u64)
                .query(&mut conn)
                .expect("set should succeed");
            let _: u64 = redis::cmd("INCR")
                .arg(&epoch_key)
                .query(&mut conn)
                .expect("incr should succeed");
            // B releases (failed quorum cleanup)
        }
        clear_lease_on_nodes(&[redis_c.redis_url()], &lease_key);

        // Node C now has epoch=1 but no lock owner
        let epoch_c_before = read_epoch(&redis_c.redis_url(), &epoch_key);

        // Leader A acquires on all free nodes (A,B,C all free now)
        let adapter_a = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter_a
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
        );
        let epoch_a = (*adapter_a.current_epoch_token.lock().expect("lock"))
            .expect("epoch should be set");

        // A's epoch should be max across all 3 nodes
        // Node C had epoch=1 from B's INCR, then A's promote INCR'd it to 2
        // Nodes A,B were at 0, A's promote INCR'd them to 1
        // A takes max(1, 1, 2) = 2
        assert!(
            epoch_a > epoch_c_before,
            "Leader's epoch ({epoch_a}) should be > node C's pre-acquisition epoch ({epoch_c_before})"
        );

        // Verify writes succeed on ALL nodes with the adopted epoch
        let block = poa_block_at_time(1, 100);
        adapter_a
            .publish_produced_block(&block)
            .expect("publish should succeed on all 3 nodes");

        assert_eq!(stream_len(&redis_a.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_b.redis_url(), &stream_key), 1);
        assert_eq!(stream_len(&redis_c.redis_url(), &stream_key), 1);
    }

    /// Exercises promotion, block write, fencing rejection, repair, and
    /// reconciliation, then dumps `encode_metrics()` to verify all PoA
    /// metrics appear on the /v1/metrics endpoint with expected values.
    #[tokio::test(flavor = "multi_thread")]
    async fn metrics__poa_metrics_appear_in_encoded_output_after_exercising_all_paths() {
        // --- setup: 3 Redis nodes ---
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:metrics-smoke".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        // 1. Promotion (success path)
        let adapter = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "adapter should acquire lease"
        );

        // 2. Successful block write
        let block1 = poa_block_at_time(1, 100);
        adapter
            .publish_produced_block(&block1)
            .expect("publish should succeed");

        // 3. HEIGHT_EXISTS — write same height again
        let block1_dup = poa_block_at_time(1, 200);
        let dup_result = adapter.publish_produced_block(&block1_dup);
        assert!(dup_result.is_err(), "duplicate height should fail");

        // 4. Fencing rejection — old leader tries to write after handoff
        let old_adapter = new_test_adapter(redis_urls.clone(), lease_key.clone());
        // Give old_adapter a stale epoch so it thinks it's leader
        {
            let mut epoch = old_adapter.current_epoch_token.lock().expect("lock");
            *epoch = Some(1);
        }
        let _zombie = old_adapter.publish_produced_block(&poa_block_at_time(2, 300));

        // 5. Reconciliation with sub-quorum repair
        //    Put an orphan block on node A only at height 2
        let orphan = poa_block_at_time(2, 400);
        let orphan_data = postcard::to_allocvec(&orphan).expect("serialize orphan");
        let client_a = redis::Client::open(redis_a.redis_url()).expect("redis client");
        let mut conn_a = client_a.get_connection().expect("redis connection");
        let epoch_val =
            (*adapter.current_epoch_token.lock().expect("lock")).expect("epoch set");
        #[allow(clippy::cast_possible_truncation)]
        let epoch_u32 = epoch_val as u32;
        append_stream_block(&mut conn_a, &stream_key, 2, &orphan_data, epoch_u32);

        // 6. leader_state triggers reconciliation + repair
        let state = adapter
            .leader_state(2.into())
            .await
            .expect("leader_state should succeed");
        assert!(
            matches!(
                state,
                LeaderState::UnreconciledBlocks(ref blocks) if !blocks.is_empty()
            ),
            "Should have unreconciled blocks: {state:?}"
        );

        // --- encode and verify ---
        let encoded =
            fuel_core_metrics::encode_metrics().expect("encode_metrics should succeed");

        // Print full output for visual inspection
        let poa_lines: Vec<&str> =
            encoded.lines().filter(|l| l.contains("poa_")).collect();
        for line in &poa_lines {
            eprintln!("{line}");
        }

        // Verify all metric names appear.
        // Counters get `_total` appended by prometheus-client automatically.
        let expected_names = [
            "poa_leader_epoch",
            "poa_is_leader",
            "poa_epoch_max_drift",
            "poa_stream_trim_headroom",
            "poa_write_block_success_total",
            "poa_write_block_height_exists_total",
            "poa_write_block_fencing_error_total",
            "poa_write_block_error_total",
            "poa_repair_success_total",
            "poa_promotion_success_total",
            "poa_promotion_duration_s",
            "poa_write_block_duration_s",
            "poa_reconciliation_duration_s",
            "poa_connection_reset_total",
        ];
        for name in &expected_names {
            assert!(
                encoded.contains(name),
                "Metric '{name}' missing from /v1/metrics output"
            );
        }

        // Verify key metrics have non-zero values.
        // For counters, the data line is e.g. `poa_write_block_success_total 3`.
        // For gauges, it's e.g. `poa_leader_epoch 2`.
        // We find the line that starts with the name, excluding sub-metric
        // lines (like `_bucket`, `_sum`, `_count`).
        let non_zero_metrics = [
            "poa_leader_epoch",
            "poa_is_leader",
            "poa_write_block_success_total",
            "poa_promotion_success_total",
            "poa_repair_success_total",
        ];
        for name in &non_zero_metrics {
            let metric_line = encoded
                .lines()
                .find(|l| {
                    l.starts_with(name)
                        && !l.starts_with(&format!("{name}_"))
                        && !l.starts_with('#')
                })
                .unwrap_or_else(|| panic!("No data line for {name}"));
            assert!(
                !metric_line.ends_with(" 0"),
                "Metric '{name}' should be non-zero, got: {metric_line}"
            );
        }
    }

    /// When quorum reads fail during reconciliation, a subsequent call
    /// should still be able to read the same backlog entries.
    #[tokio::test(flavor = "multi_thread")]
    async fn unreconciled_blocks__after_quorum_read_failure_then_backlog_remains_readable()
     {
        // given — 3 Redis nodes, block published to all 3
        let mut redis_a = RedisTestServer::spawn();
        let mut redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:cursor-restore-quorum".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        let adapter = new_test_adapter(redis_urls.clone(), lease_key.clone());
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Should acquire lease"
        );

        let block = poa_block_at_time(1, 100);
        adapter
            .publish_produced_block(&block)
            .expect("publish should succeed on all 3 nodes");
        adapter.release().await.expect("release should succeed");

        // when — kill 2 nodes so quorum read fails
        redis_a.stop();
        redis_b.stop();

        let adapter_b = new_test_adapter(redis_urls.clone(), lease_key.clone());
        {
            let mut epoch = adapter_b.current_epoch_token.lock().expect("lock");
            *epoch = Some(99);
        }
        let result = adapter_b.unreconciled_blocks(1.into()).await;
        assert!(result.is_err(), "Should fail with quorum read failure");

        // Restart the killed nodes — all 3 now reachable
        redis_a.start();
        redis_b.start();

        // Re-publish block to the restarted nodes so they have data
        let client_a = redis::Client::open(redis_a.redis_url()).expect("client");
        let mut conn_a = client_a.get_connection().expect("conn");
        let client_b = redis::Client::open(redis_b.redis_url()).expect("client");
        let mut conn_b = client_b.get_connection().expect("conn");
        let block_data = postcard::to_allocvec(&block).expect("serialize");
        append_stream_block(&mut conn_a, &stream_key, 1, &block_data, 1);
        append_stream_block(&mut conn_b, &stream_key, 1, &block_data, 1);

        // then — subsequent call must still see the block on node C
        let blocks = adapter_b
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation should succeed now");
        assert_eq!(
            blocks.len(),
            1,
            "Quorum read failure should not make backlog entries unreadable"
        );
    }

    /// When sub-quorum repair fails, the next reconciliation round
    /// should still be able to re-read and retry.
    #[tokio::test(flavor = "multi_thread")]
    async fn unreconciled_blocks__after_repair_failure_then_backlog_remains_readable() {
        // given — 3 Redis nodes, block published to only 1 node (sub-quorum)
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:cursor-restore-repair".to_string();
        let stream_key = format!("{lease_key}:block:stream");
        let redis_urls = vec![
            redis_a.redis_url(),
            redis_b.redis_url(),
            redis_c.redis_url(),
        ];

        let block = poa_block_at_time(1, 100);
        let block_data = postcard::to_allocvec(&block).expect("serialize");

        // Write block to only node A — sub-quorum (1/3)
        let client_a = redis::Client::open(redis_a.redis_url()).expect("client");
        let mut conn_a = client_a.get_connection().expect("conn");
        append_stream_block(&mut conn_a, &stream_key, 1, &block_data, 1);

        // Adapter without a lease — repair will fail (no lock held)
        let adapter = new_test_adapter(redis_urls.clone(), lease_key.clone());
        // Set epoch but do NOT acquire lease — repair_sub_quorum_block
        // will get FencingRejected or fail to reach quorum
        {
            let mut epoch = adapter.current_epoch_token.lock().expect("lock");
            *epoch = Some(99);
        }

        // First call reads entries, then repair fails because we don't hold the lock.
        let result = adapter.unreconciled_blocks(1.into()).await;
        // Repair failure now returns an error because backlog remains unresolved.
        assert!(
            result.is_err(),
            "Should return error when repair fails and backlog remains unresolved"
        );

        // Now acquire the lease so repair can succeed
        assert!(
            adapter
                .acquire_lease_if_free()
                .await
                .expect("acquire should succeed"),
            "Should acquire lease"
        );

        // then — second call must still see the sub-quorum block
        let blocks = adapter
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation should succeed with lock held");
        assert_eq!(
            blocks.len(),
            1,
            "Repair failure should not make backlog unreadable on the next round"
        );
    }
}
