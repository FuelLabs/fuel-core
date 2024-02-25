use super::{
    BlockImporterAdapter,
    BlockProducerAdapter,
};
use crate::{
    database::Database,
    fuel_core_graphql_api::ports::{
        worker,
        BlockProducerPort,
        DatabaseMessageProof,
        P2pPort,
        TxPoolPort,
    },
    service::adapters::{
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
    entities::message::MerkleProof,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::{
        block_importer::SharedImportResult,
        executor::TransactionExecutionStatus,
        p2p::PeerInfo,
        txpool::InsertionResult,
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
            .map(|res| res.map_err(anyhow::Error::from))
            .collect()
    }

    fn tx_update_subscribe(
        &self,
        id: TxId,
    ) -> anyhow::Result<BoxStream<TxStatusMessage>> {
        self.service.tx_update_subscribe(id)
    }
}

impl DatabaseMessageProof for Database {
    fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof> {
        Database::block_history_proof(self, message_block_height, commit_block_height)
    }
}

#[async_trait]
impl BlockProducerPort for BlockProducerAdapter {
    async fn dry_run_txs(
        &self,
        transactions: Vec<Transaction>,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<TransactionExecutionStatus>> {
        self.block_producer
            .dry_run(transactions, height, utxo_validation)
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

impl worker::BlockImporter for BlockImporterAdapter {
    fn block_events(&self) -> BoxStream<SharedImportResult> {
        self.events()
    }
}
