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
use fuel_core_poa::{
    ports::{
        BlockImporter,
        LeaderLeasePort,
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
    blockchain::block::Block,
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
use std::path::{
    Path,
    PathBuf,
};
use std::time::Duration;
use tokio::{
    sync::watch,
    time::Instant,
};
use tokio_stream::{
    StreamExt,
    wrappers::BroadcastStream,
};

pub mod pre_confirmation_signature;

#[derive(Clone)]
pub struct RedisLeaderLeaseAdapter {
    redis_client: redis::Client,
    lease_key: String,
    lease_owner_token: String,
    lease_ttl_millis: u64,
}

impl RedisLeaderLeaseAdapter {
    pub fn new(
        redis_url: String,
        lease_key: String,
        lease_ttl: Duration,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !redis_url.is_empty(),
            "Redis producer failover url must not be empty"
        );
        anyhow::ensure!(
            !lease_key.is_empty(),
            "Redis producer failover lease_key must not be empty"
        );
        anyhow::ensure!(
            lease_ttl > Duration::ZERO,
            "Redis producer failover lease_ttl must be greater than zero"
        );
        let redis_client = redis::Client::open(redis_url)?;
        let lease_ttl_millis = lease_ttl.as_millis() as u64;
        let lease_owner_token = uuid::Uuid::new_v4().to_string();
        Ok(Self {
            redis_client,
            lease_key,
            lease_owner_token,
            lease_ttl_millis,
        })
    }

    async fn renew_lease_if_owner(&self) -> anyhow::Result<bool> {
        let mut connection = self.redis_client.get_multiplexed_async_connection().await?;
        let renewed: i32 = redis::Script::new(
            "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                return redis.call('PEXPIRE', KEYS[1], ARGV[2]) \
            else \
                return 0 \
            end",
        )
        .key(&self.lease_key)
        .arg(&self.lease_owner_token)
        .arg(self.lease_ttl_millis)
        .invoke_async(&mut connection)
        .await?;
        Ok(renewed == 1)
    }

    async fn acquire_lease_if_free(&self) -> anyhow::Result<bool> {
        let mut connection = self.redis_client.get_multiplexed_async_connection().await?;
        let acquired: i32 = redis::Script::new(
            "if redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[2], 'NX') then \
                return 1 \
            else \
                return 0 \
            end",
        )
        .key(&self.lease_key)
        .arg(&self.lease_owner_token)
        .arg(self.lease_ttl_millis)
        .invoke_async(&mut connection)
        .await?;
        Ok(acquired == 1)
    }

    async fn release_lease_if_owner(&self) -> anyhow::Result<bool> {
        let mut connection = self.redis_client.get_multiplexed_async_connection().await?;
        let released: i32 = redis::Script::new(
            "if redis.call('GET', KEYS[1]) == ARGV[1] then \
                return redis.call('DEL', KEYS[1]) \
            else \
                return 0 \
            end",
        )
        .key(&self.lease_key)
        .arg(&self.lease_owner_token)
        .invoke_async(&mut connection)
        .await?;
        Ok(released == 1)
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
impl LeaderLeasePort for RedisLeaderLeaseAdapter {
    async fn can_produce_block(&self, _height: BlockHeight) -> anyhow::Result<bool> {
        if self.renew_lease_if_owner().await? {
            return Ok(true);
        }
        self.acquire_lease_if_free().await
    }

    async fn release_lease(&self) -> anyhow::Result<()> {
        let _ = self.release_lease_if_owner().await?;
        Ok(())
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
