use super::{
    BlockImporterAdapter,
    BlockProducerAdapter,
    ConsensusParametersProvider,
    StaticGasPrice,
};
use crate::{
    database::OnChainIterableKeyValueView,
    fuel_core_graphql_api::ports::{
        worker,
        BlockProducerPort,
        ConsensusProvider,
        DatabaseMessageProof,
        GasPriceEstimate,
        P2pPort,
        TxPoolPort,
    },
    service::adapters::{
        import_result_provider::ImportResultProvider,
        P2PAdapter,
        TxPoolAdapter,
    },
};
use async_trait::async_trait;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_txpool::{
    service::TxStatusMessage,
    types::TxId,
};
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    entities::relayer::message::MerkleProof,
    fuel_tx::{
        Bytes32,
        ConsensusParameters,
        Transaction,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::SharedImportResult,
        executor::TransactionExecutionStatus,
        p2p::PeerInfo,
        txpool::{
            InsertionResult,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use std::{
    ops::Deref,
    sync::Arc,
};

mod off_chain;
mod on_chain;

#[async_trait]
impl TxPoolPort for TxPoolAdapter {
    fn transaction(&self, id: TxId) -> Option<Transaction> {
        self.service
            .find_one(id)
            .map(|info| info.tx().clone().deref().into())
    }

    fn submission_time(&self, id: TxId) -> Option<Tai64> {
        self.service
            .find_one(id)
            .map(|info| Tai64::from_unix(info.submitted_time().as_secs() as i64))
    }

    async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> Vec<anyhow::Result<InsertionResult>> {
        self.service
            .insert(txs)
            .await
            .into_iter()
            .map(|res| res.map_err(|e| anyhow::anyhow!(e)))
            .collect()
    }

    fn tx_update_subscribe(
        &self,
        id: TxId,
    ) -> anyhow::Result<BoxStream<TxStatusMessage>> {
        self.service.tx_update_subscribe(id)
    }
}

impl DatabaseMessageProof for OnChainIterableKeyValueView {
    fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof> {
        self.block_history_proof(message_block_height, commit_block_height)
    }
}

#[async_trait]
impl BlockProducerPort for BlockProducerAdapter {
    async fn dry_run_txs(
        &self,
        transactions: Vec<Transaction>,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
        gas_price: Option<u64>,
    ) -> anyhow::Result<Vec<TransactionExecutionStatus>> {
        self.block_producer
            .dry_run(transactions, height, utxo_validation, gas_price)
            .await
    }
}

#[async_trait::async_trait]
impl P2pPort for P2PAdapter {
    async fn all_peer_info(&self) -> anyhow::Result<Vec<PeerInfo>> {
        #[cfg(feature = "p2p")]
        {
            use fuel_core_types::services::p2p::HeartbeatData;
            if let Some(service) = &self.service {
                let peers = service.get_all_peers().await?;
                Ok(peers
                    .into_iter()
                    .map(|(peer_id, peer_info)| PeerInfo {
                        id: fuel_core_types::services::p2p::PeerId::from(
                            peer_id.to_bytes(),
                        ),
                        peer_addresses: peer_info
                            .peer_addresses
                            .iter()
                            .map(|addr| addr.to_string())
                            .collect(),
                        client_version: None,
                        heartbeat_data: HeartbeatData {
                            block_height: peer_info.heartbeat_data.block_height,
                            last_heartbeat: peer_info.heartbeat_data.last_heartbeat_sys,
                        },
                        app_score: peer_info.score,
                    })
                    .collect())
            } else {
                Ok(vec![])
            }
        }
        #[cfg(not(feature = "p2p"))]
        {
            Ok(vec![])
        }
    }
}

impl worker::TxPool for TxPoolAdapter {
    fn send_complete(
        &self,
        id: Bytes32,
        block_height: &BlockHeight,
        status: TransactionStatus,
    ) {
        self.service.send_complete(id, block_height, status)
    }
}

#[async_trait::async_trait]
impl GasPriceEstimate for StaticGasPrice {
    async fn worst_case_gas_price(&self, _height: BlockHeight) -> Option<u64> {
        Some(self.gas_price)
    }
}

impl ConsensusProvider for ConsensusParametersProvider {
    fn latest_consensus_params(&self) -> Arc<ConsensusParameters> {
        self.shared_state.latest_consensus_parameters()
    }

    fn consensus_params_at_version(
        &self,
        version: &ConsensusParametersVersion,
    ) -> anyhow::Result<Arc<ConsensusParameters>> {
        Ok(self.shared_state.get_consensus_parameters(version)?)
    }
}

#[derive(Clone)]
pub struct GraphQLBlockImporter {
    block_importer_adapter: BlockImporterAdapter,
    import_result_provider_adapter: ImportResultProvider,
}

impl GraphQLBlockImporter {
    pub fn new(
        block_importer_adapter: BlockImporterAdapter,
        import_result_provider_adapter: ImportResultProvider,
    ) -> Self {
        Self {
            block_importer_adapter,
            import_result_provider_adapter,
        }
    }
}

impl worker::BlockImporter for GraphQLBlockImporter {
    fn block_events(&self) -> BoxStream<SharedImportResult> {
        self.block_importer_adapter.events_shared_result()
    }

    fn block_event_at_height(
        &self,
        height: Option<BlockHeight>,
    ) -> anyhow::Result<SharedImportResult> {
        self.import_result_provider_adapter.result_at_height(height)
    }
}
