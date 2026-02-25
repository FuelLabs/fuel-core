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
        LeaderState,
        P2pPort,
        PredefinedBlocks,
        BlockReconciliationReadPort,
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
    current_epoch_token: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    lease_ttl_millis: u64,
    lease_drift_millis: u64,
    node_timeout: Duration,
    retry_delay_millis: u64,
    max_retry_delay_offset_millis: u64,
    max_attempts: usize,
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
            current_epoch_token: self.current_epoch_token.clone(),
            lease_ttl_millis: self.lease_ttl_millis,
            lease_drift_millis: self.lease_drift_millis,
            node_timeout: self.node_timeout,
            retry_delay_millis: self.retry_delay_millis,
            max_retry_delay_offset_millis: self.max_retry_delay_offset_millis,
            max_attempts: self.max_attempts,
        }
    }
}

#[derive(Default, Clone)]
pub struct NoopReconciliationAdapter;

pub enum ReconciliationAdapter {
    Redis(RedisLeaderLeaseAdapter),
    Noop(NoopReconciliationAdapter),
}

impl RedisLeaderLeaseAdapter {
    pub fn new(
        redis_urls: Vec<String>,
        lease_key: String,
        lease_ttl: Duration,
        node_timeout: Duration,
        retry_delay: Duration,
        max_retry_delay_offset: Duration,
        max_attempts: u32,
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
            current_epoch_token: std::sync::Arc::new(std::sync::Mutex::new(None)),
            lease_ttl_millis,
            lease_drift_millis,
            node_timeout,
            retry_delay_millis,
            max_retry_delay_offset_millis,
            max_attempts,
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
            redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                    return 1 \
                else \
                    return 0 \
                end",
            )
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
            redis::Script::new(
                "local acquired = redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[2], 'NX') \
                if not acquired then \
                    return redis.error_reply('LOCK_HELD: Another leader holds the lock') \
                end \
                local new_token = redis.call('INCR', KEYS[2]) \
                return new_token",
            )
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
            redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                    return redis.call('DEL', KEYS[1]) \
                else \
                    return 0 \
                end",
            )
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

    async fn renew_lease_if_owner(&self) -> anyhow::Result<bool> {
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
                    let mut current_epoch_token =
                        self.current_epoch_token.lock().expect("poisoned lock");
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
            redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                    return redis.call('DEL', KEYS[1]) \
                else \
                    return 0 \
                end",
            )
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
            let _ = redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                    return redis.call('DEL', KEYS[1]) \
                else \
                    return 0 \
                end",
            )
            .key(&lease_key)
            .arg(&lease_owner_token)
            .invoke::<i32>(&mut connection);
        });
    }

    async fn can_produce_block(&self, _height: BlockHeight) -> anyhow::Result<bool> {
        tracing::debug!("Checking Redis leader lock");
        if self.renew_lease_if_owner().await? {
            return Ok(true);
        }
        self.acquire_lease_if_free().await
    }

    async fn release_if_owner(&self) -> anyhow::Result<()> {
        tracing::debug!("Releasing Redis leader lock");
        let releases = futures::future::join_all(
            self.redis_nodes
                .iter()
                .map(|redis_node| self.release_lease_on_node(redis_node)),
        )
        .await;
        let released_count = releases.into_iter().filter(|released| *released).count();
        if self.quorum_reached(released_count) {
            let mut current_epoch_token =
                self.current_epoch_token.lock().expect("poisoned lock");
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
        let mut connection = redis_node.redis_client.get_connection()?;
        let block_height = u32::from(*block.entity.header().height());
        let write_result = redis::Script::new(
            "local current_token = tonumber(redis.call('GET', KEYS[2]) or '0') \
            local current_leader = redis.call('GET', KEYS[3]) \
            if current_leader ~= ARGV[2] then \
                return redis.error_reply('FENCING_ERROR: Lock lost or held by another node') \
            end \
            if tonumber(ARGV[1]) < current_token then \
                return redis.error_reply('FENCING_ERROR: Token is stale') \
            end \
            if tonumber(ARGV[1]) > current_token then \
                redis.call('SET', KEYS[2], ARGV[1]) \
            end \
            local stream_id = redis.call('XADD', KEYS[1], '*', \
                'height', ARGV[3], \
                'data', ARGV[4], \
                'epoch', ARGV[1], \
                'timestamp', redis.call('TIME')[1]) \
            redis.call('PEXPIRE', KEYS[3], ARGV[5]) \
            return stream_id",
        )
        .key(&self.block_stream_key)
        .key(&self.epoch_key)
        .key(&self.lease_key)
        .arg(epoch)
        .arg(&self.lease_owner_token)
        .arg(block_height)
        .arg(block_data)
        .arg(self.lease_ttl_millis)
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
        _local_height: BlockHeight,
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
        _local_height: BlockHeight,
        next_height: BlockHeight,
    ) -> anyhow::Result<LeaderState> {
        if self.can_produce_block(next_height).await? {
            Ok(LeaderState::ReconciledLeader)
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
        local_height: BlockHeight,
        next_height: BlockHeight,
    ) -> anyhow::Result<LeaderState> {
        match self {
            Self::Redis(adapter) => adapter.leader_state(local_height, next_height).await,
            Self::Noop(adapter) => adapter.leader_state(local_height, next_height).await,
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
        let epoch = match *self.current_epoch_token.lock().expect("poisoned lock") {
            Some(epoch) => epoch,
            None => {
                tracing::debug!(
                    "Skipping redis block publish because fencing token is not initialized"
                );
                return Ok(());
            }
        };
        let block_data = postcard::to_allocvec(block)?;
        let successes = self
            .redis_nodes
            .iter()
            .map(|redis_node| match self.publish_block_on_node(
                redis_node,
                epoch,
                block,
                &block_data,
            ) {
                Ok(success) => success,
                Err(err) => {
                    tracing::debug!("Redis publish on node failed: {err}");
                    false
                }
            })
            .into_iter()
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

#[cfg(feature = "p2p")]
impl P2pPort for P2PAdapter {
    fn reserved_peers_count(&self) -> BoxStream<usize> {
        match &self.service {
            Some(service) => Box::pin(
                BroadcastStream::new(service.subscribe_reserved_peers_count())
                    .filter_map(|result| result.ok()),
            ),
            _ => Box::pin(tokio_stream::pending()),
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
    path_to_directory.join(format!("{}.json", block_height))
}

pub fn block_exists(path_to_directory: &Path, block_height: u32) -> bool {
    block_path(path_to_directory, block_height).exists()
}
