use crate::{
    fuel_core_graphql_api::ports::ConsensusModulePort,
    service::adapters::{
        BlockImporterAdapter,
        BlockProducerAdapter,
        P2PAdapter,
        PoAAdapter,
        SharedSequencerAdapter,
        TxPoolAdapter,
    },
};
use anyhow::anyhow;
use fuel_core_poa::{
    ports::{
        BlockImporter,
        P2pPort,
        SharedSequencerPort,
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
        primitives::SecretKeyWrapper,
        SealedBlock,
    },
    fuel_tx::TxId,
    fuel_types::BlockHeight,
    secrecy::Secret,
    services::{
        block_importer::{
            BlockImportInfo,
            UncommittedResult as UncommittedImporterResult,
        },
        executor::{
            Error as ExecutorError,
            UncommittedResult,
        },
        txpool::ArcPoolTx,
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
    fn pending_number(&self) -> usize {
        self.service.pending_number()
    }

    fn total_consumable_gas(&self) -> u64 {
        self.service.total_consumable_gas()
    }

    fn remove_txs(&self, ids: Vec<(TxId, ExecutorError)>) -> Vec<ArcPoolTx> {
        self.service.remove_txs(
            ids.into_iter()
                .map(|(tx_id, err)| (tx_id, err.to_string()))
                .collect(),
        )
    }

    fn transaction_status_events(&self) -> BoxStream<TxId> {
        Box::pin(
            BroadcastStream::new(self.service.new_tx_notification_subscribe())
                .filter_map(|result| result.ok()),
        )
    }
}

#[async_trait::async_trait]
impl fuel_core_poa::ports::BlockProducer for BlockProducerAdapter {
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        match source {
            TransactionsSource::TxPool => {
                self.block_producer
                    .produce_and_execute_block_txpool(height, block_time)
                    .await
            }
            TransactionsSource::SpecificTransactions(txs) => {
                self.block_producer
                    .produce_and_execute_block_transactions(height, block_time, txs)
                    .await
            }
        }
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
                .map(BlockImportInfo::from),
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

#[async_trait::async_trait]
impl SharedSequencerPort for SharedSequencerAdapter {
    #[cfg(feature = "shared-sequencer")]
    async fn send(
        &mut self,
        signing_key: &Secret<SecretKeyWrapper>,
        block: SealedBlock,
    ) -> anyhow::Result<()> {
        use fuel_core_shared_sequencer_client::SigningKey;
        use fuel_core_types::secrecy::ExposeSecret;
        // TODO: zeroize the key on drop
        let sk = SigningKey::from_slice(&***signing_key.expose_secret())
            .expect("the key conversion never fails");
        self.client.lock().await.send(&sk, block).await
    }

    #[cfg(not(feature = "shared-sequencer"))]
    async fn send(
        &mut self,
        _signing_key: &Secret<SecretKeyWrapper>,
        _block: SealedBlock,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
