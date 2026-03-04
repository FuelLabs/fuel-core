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
use fuel_core_importer::ports::BlockReconciliationWritePort;
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
        let quorum = redis_nodes
            .len()
            .checked_div(2)
            .unwrap_or(0)
            .saturating_add(1);
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
        let owner_count = ownership.into_iter().filter(|is_owner| *is_owner).count();
        Ok(self.quorum_reached(owner_count))
    }

    async fn acquire_lease_if_free(&self) -> anyhow::Result<bool> {
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
                if let Some(max_token) = promoted_tokens.into_iter().max() {
                    let mut current_epoch_token = self
                        .current_epoch_token
                        .lock()
                        .map_err(|e| anyhow!("epoch token lock poisoned: {}", e))?;
                    *current_epoch_token = Some(max_token);
                }
                return Ok(true);
            }
            self.release_lease_on_all_nodes().await;
            let is_last_attempt = attempt_index.saturating_add(1) == self.max_attempts;
            if !is_last_attempt {
                self.delay_next_retry().await;
            }
        }
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

    async fn read_stream_entries_on_node(
        &self,
        redis_node: &RedisNode,
    ) -> Vec<(u32, u64, SealedBlock)> {
        let mut connection = match self.multiplexed_connection(redis_node).await {
            Ok(connection) => connection,
            Err(_) => return Vec::new(),
        };
        let stream_entries = timeout(
            self.node_timeout,
            redis::Script::new(READ_STREAM_ENTRIES_SCRIPT)
                .key(&self.block_stream_key)
                .invoke_async::<Vec<(u32, u64, Vec<u8>)>>(&mut connection),
        )
        .await;
        let stream_entries = match stream_entries {
            Ok(Ok(stream_entries)) => stream_entries,
            Ok(Err(_)) | Err(_) => {
                self.clear_cached_connection(redis_node).await;
                return Vec::new();
            }
        };
        stream_entries
            .into_iter()
            .filter_map(|(height, epoch, bytes)| {
                postcard::from_bytes::<SealedBlock>(&bytes)
                    .ok()
                    .map(|block| (height, epoch, block))
            })
            .collect()
    }

    async fn unreconciled_blocks(
        &self,
        next_height: BlockHeight,
    ) -> anyhow::Result<Vec<SealedBlock>> {
        let mut reconciled = Vec::new();
        let max_reconcile_blocks_per_round =
            usize::try_from(self.stream_max_len).unwrap_or(usize::MAX);
        let blocks_by_node = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.read_stream_entries_on_node(redis_node)),
        )
        .await
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
        let mut current_height = u32::from(next_height);

        for _ in 0..max_reconcile_blocks_per_round {
            let nodes_with_height = blocks_by_node
                .iter()
                .filter(|blocks_by_height| blocks_by_height.contains_key(&current_height))
                .count();

            if !self.quorum_reached(nodes_with_height) {
                break;
            }

            let votes = blocks_by_node
                .iter()
                .filter_map(|blocks_by_height| blocks_by_height.get(&current_height))
                .flat_map(|blocks_by_epoch| blocks_by_epoch.iter())
                .fold(
                    HashMap::<(u64, BlockId), (usize, SealedBlock)>::new(),
                    |mut votes, (epoch, block)| {
                        let vote_key = (*epoch, block.entity.id());
                        match votes.get_mut(&vote_key) {
                            Some((count, _)) => {
                                *count = count.saturating_add(1);
                            }
                            None => {
                                votes.insert(vote_key, (1, block.clone()));
                            }
                        }
                        votes
                    },
                );

            let winner = votes
                .into_iter()
                .filter(|(_, (count, _))| self.quorum_reached(*count))
                .max_by_key(|((epoch, _), _)| *epoch)
                .map(|(_, (_, block))| block);
            if let Some(block) = winner {
                reconciled.push(block);
            } else {
                break;
            }

            let Some(next) = current_height.checked_add(1) else {
                break;
            };
            current_height = next;
        }

        Ok(reconciled)
    }

    async fn can_produce_block(&self, _height: BlockHeight) -> anyhow::Result<bool> {
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

    fn publish_block_on_node(
        &self,
        redis_node: &RedisNode,
        epoch: u64,
        block: &SealedBlock,
        block_data: &[u8],
    ) -> anyhow::Result<bool> {
        let mut connection = redis_node
            .redis_client
            .get_connection_with_timeout(self.node_timeout)?;
        connection.set_read_timeout(Some(self.node_timeout))?;
        connection.set_write_timeout(Some(self.node_timeout))?;
        let block_height = u32::from(*block.entity.header().height());
        let write_result = redis::Script::new(WRITE_BLOCK_SCRIPT)
            .key(&self.block_stream_key)
            .key(&self.epoch_key)
            .key(&self.lease_key)
            .arg(epoch)
            .arg(&self.lease_owner_token)
            .arg(block_height)
            .arg(block_data)
            .arg(self.lease_ttl_millis)
            .arg(self.stream_max_len)
            .invoke::<String>(&mut connection);
        match write_result {
            Ok(_) => Ok(true),
            Err(err) if err.to_string().contains("FENCING_ERROR:") => Ok(false),
            Err(err) => Err(err.into()),
        }
    }
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
        if self.can_produce_block(next_height).await? {
            let unreconciled_blocks = self.unreconciled_blocks(next_height).await?;
            if unreconciled_blocks.is_empty() {
                Ok(LeaderState::ReconciledLeader)
            } else {
                Ok(LeaderState::UnreconciledBlocks(unreconciled_blocks))
            }
        } else {
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
            .redis_nodes
            .iter()
            .map(|redis_node| {
                match self.publish_block_on_node(redis_node, epoch, block, &block_data) {
                    Ok(success) => success,
                    Err(err) => {
                        tracing::debug!("Redis publish on node failed: {err}");
                        false
                    }
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
}

#[cfg(all(test, feature = "leader_lock", not(feature = "not_leader_lock")))]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use fuel_core_importer::ports::BlockReconciliationWritePort;
    use fuel_core_poa::ports::BlockReconciliationReadPort;
    use fuel_core_types::blockchain::consensus::Consensus;
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
        time::Duration,
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
    async fn leader_state__when_height_has_quorum_epoch_but_block_ids_disagree_then_does_not_reconcile_height()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:epoch-quorum-block-mismatch".to_string();
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

        append_stream_block(&mut conn_a, &stream_key, 1, &block_a_data, 7);
        append_stream_block(&mut conn_b, &stream_key, 1, &block_b_data, 7);

        // when
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then
        assert!(
            matches!(leader_state, LeaderState::ReconciledLeader),
            "Expected reconciliation to reject epoch quorum without block-id quorum",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_same_height_entry_exists_on_less_than_quorum_nodes_then_ignores_it()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:below-quorum".to_string();
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

        let orphan_block = poa_block_at_time(1, 10);
        let orphan_block_data =
            postcard::to_allocvec(&orphan_block).expect("orphan block should serialize");

        let redis_client =
            redis::Client::open(redis_a.redis_url()).expect("redis client should open");
        let mut conn = redis_client
            .get_connection()
            .expect("redis connection should open");
        append_stream_block(&mut conn, &stream_key, 1, &orphan_block_data, 1);

        // when
        let leader_state = adapter
            .leader_state(1.into())
            .await
            .expect("leader_state should succeed");

        // then
        assert!(
            matches!(leader_state, LeaderState::ReconciledLeader),
            "Expected below-quorum entry to be ignored"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn leader_state__when_contiguous_heights_have_quorum_then_returns_blocks_until_first_non_quorum_height()
     {
        // given
        let redis_a = RedisTestServer::spawn();
        let redis_b = RedisTestServer::spawn();
        let redis_c = RedisTestServer::spawn();
        let lease_key = "poa:test:contiguous-quorum".to_string();
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

        // h1 on quorum (a,b)
        append_stream_block(&mut conn_a, &stream_key, 1, &h1_data, 1);
        append_stream_block(&mut conn_b, &stream_key, 1, &h1_data, 1);
        // h2 on quorum (a,b)
        append_stream_block(&mut conn_a, &stream_key, 2, &h2_data, 1);
        append_stream_block(&mut conn_b, &stream_key, 2, &h2_data, 1);
        // h3 below quorum (a only)
        append_stream_block(&mut conn_a, &stream_key, 3, &h3_data, 1);
        let _ = &mut conn_c;

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
            unreconciled_blocks
                .iter()
                .map(|b| u32::from(*b.entity.header().height()))
                .collect::<Vec<_>>(),
            vec![1, 2],
            "Expected only contiguous quorum-backed heights to be reconciled",
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
        let unreconciled = adapter
            .unreconciled_blocks(1.into())
            .await
            .expect("reconciliation read should succeed");

        // then
        assert!(
            publish_result.is_err(),
            "Publish must fail when fewer than quorum nodes accept write"
        );
        assert!(
            unreconciled.is_empty(),
            "Below-quorum stream entry must not be reconciled as committed"
        );
        assert_eq!(
            stream_len(&redis_a.redis_url(), &stream_key),
            1,
            "One orphan entry should exist on the single successful node"
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
}
