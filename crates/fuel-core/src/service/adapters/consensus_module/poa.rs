use crate::{
    database::Database,
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
        P2pPort,
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
use tokio_stream::{
    wrappers::BroadcastStream,
    StreamExt,
};

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

    fn block_stream(&self) -> BoxStream<BlockHeight> {
        Box::pin(
            BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|result| result.ok())
                .map(|r| *r.sealed_block.entity.header().height()),
        )
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

// `ImportResult` add new field to identify is the block produce by ourself or from the network
// P2P provides subscription to the "current number of connected reserved peers".
//
// With next statements all unit test should continue to work as before:
// If `min_connected_reserved_peers` is zero, then the initial `State` is `SufficientPeers`.
// If `time_until_synced` and `min_connected_reserved_peers` are, then the initial `State` is `Synced`.
// For the `Config::local_testnet` `min_connected_reserved_peers` and `time_until_synced` are zero.
//
// Not to forget to add a new integration test where we start teh second block producer.
