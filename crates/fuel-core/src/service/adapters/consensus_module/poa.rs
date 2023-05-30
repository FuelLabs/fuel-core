use crate::{
    database::Database,
    fuel_core_graphql_api::ports::ConsensusModulePort,
    service::adapters::{
        BlockImporterAdapter,
        BlockProducerAdapter,
        NetworkInfo,
        P2PAdapter,
        PoAAdapter,
        TxPoolAdapter,
    },
};
use anyhow::anyhow;
use fuel_core_poa::{
    ports::{
        BlockImporter,
        SyncPort,
        TransactionPool,
    },
    service::SharedState,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    fuel_asm::Word,
    fuel_tx::TxId,
    fuel_types::BlockHeight,
    services::{
        block_importer::UncommittedResult as UncommittedImporterResult,
        executor::UncommittedResult,
        txpool::{
            ArcPoolTx,
            TxStatus,
        },
    },
    tai64::Tai64,
};
use tokio::time::Instant;

impl PoAAdapter {
    pub fn new(shared_state: Option<SharedState>) -> Self {
        Self { shared_state }
    }
}

#[async_trait::async_trait]
impl ConsensusModulePort for PoAAdapter {
    async fn manually_produce_blocks(
        &self,
        start_time: Option<Tai64>,
        number_of_blocks: u32,
    ) -> anyhow::Result<()> {
        self.shared_state
            .as_ref()
            .ok_or(anyhow!("The block production is disabled"))?
            .manually_produce_block(start_time, number_of_blocks)
            .await
    }
}

impl TransactionPool for TxPoolAdapter {
    fn pending_number(&self) -> usize {
        self.service.pending_number()
    }

    fn total_consumable_gas(&self) -> u64 {
        self.service.total_consumable_gas()
    }

    fn remove_txs(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.service.remove_txs(ids)
    }

    fn transaction_status_events(&self) -> BoxStream<TxStatus> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.service.tx_status_subscribe())
                .filter_map(|result| result.ok()),
        )
    }
}

#[async_trait::async_trait]
impl fuel_core_poa::ports::BlockProducer for BlockProducerAdapter {
    type Database = Database;

    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<Database>>> {
        self.block_producer
            .produce_and_execute_block(height, block_time, max_gas)
            .await
    }
}

impl BlockImporter for BlockImporterAdapter {
    type Database = Database;

    fn commit_result(
        &self,
        result: UncommittedImporterResult<StorageTransaction<Self::Database>>,
    ) -> anyhow::Result<()> {
        self.block_importer
            .commit_result(result)
            .map_err(Into::into)
    }
}

enum State {
    /// We are not connected at least to `min_connected_reserved_peers` peers.
    ///
    /// InsufficientPeers -> SufficientPeers
    InsufficientPeers(BlockHeight),
    /// We are connected to at least `min_connected_reserved_peers` peers.
    ///
    /// SufficientPeers -> Synced(...)
    SufficientPeers(BlockHeight, Instant),
    /// We can go into this state if we didn't receive any notification
    /// about new block height from the network for `time_until_synced` timeout.
    ///
    /// We can leave this state only in the case, if we received a valid block
    /// from the network with higher block height.
    ///
    /// Synced -> either InsufficientPeers(...) or SufficientPeers(...)
    Synced(BlockHeight),
}

// `ImportResult` add new field to identify is the block produce by ourself or from the network
// P2P provides subscription to the "current number of connected reserved peers".
//
// With next statements all unit test should continue to work as before:
// If `min_connected_reserved_peers` is zero, then the initial `State` is `SufficientPeers`.
// If `time_until_synced` and `min_connected_reserved_peers` are, then the initial `State` is `Synced`.
// For the `Config::local_testnet` `min_connected_reserved_peers` and `time_until_synced` are zero.
//
// Not to forget to add a new integration test where we start teh second block producer.

#[async_trait::async_trait]
impl SyncPort for crate::service::adapters::SyncAdapter<P2PAdapter> {
    async fn sync_with_peers(&mut self) -> anyhow::Result<()> {
        // if not enabled - we are considered to be synced
        if !is_p2p_enabled() {
            return Ok(())
        }

        // todo: check if the next block to be produced equals last previous block + 1
        //  if yes, then we are synced already

        // 1. check count of connected reserved peers
        // todo: add 'n' tries or a timeout
        loop {
            let count = self.network_info.connected_reserved_peers().await?;
            if count >= self.min_connected_reserved_peers {
                break
            }
            tokio::time::sleep(self.timeout_between_checking_peers).await;
        }

        // 2. receive all the blocks
        while tokio::time::timeout(self.time_until_synced, self.block_rx.recv())
            .await
            .is_ok()
        {
            // keep receiving them blocks
        }

        Ok(())
    }
}

fn is_p2p_enabled() -> bool {
    cfg!(feature = "p2p")
}
