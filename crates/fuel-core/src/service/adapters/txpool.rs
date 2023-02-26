use crate::{
    database::Database,
    service::adapters::{
        BlockImporterAdapter,
        P2PAdapter,
    },
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsRawCode,
        Messages,
        SpentMessages,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::ports::BlockImporter;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::{
        coin::CompressedCoin,
        message::CompressedMessage,
    },
    fuel_tx::{
        Transaction,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        MessageId,
    },
    services::{
        block_importer::ImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            TransactionGossipData,
        },
    },
};
use std::sync::Arc;

impl BlockImporter for BlockImporterAdapter {
    fn block_events(&self) -> BoxStream<Arc<ImportResult>> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|result| result.ok()),
        )
    }
}

#[cfg(feature = "p2p")]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()> {
        if let Some(service) = &self.service {
            service.broadcast_transaction(transaction)
        } else {
            Ok(())
        }
    }

    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        if let Some(service) = &self.service {
            Box::pin(
                BroadcastStream::new(service.subscribe_tx())
                    .filter_map(|result| result.ok()),
            )
        } else {
            fuel_core_services::stream::IntoBoxStream::into_boxed(tokio_stream::pending())
        }
    }

    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        if let Some(service) = &self.service {
            service.notify_gossip_transaction_validity(message_info, validity)
        } else {
            Ok(())
        }
    }
}

#[cfg(not(feature = "p2p"))]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    fn broadcast_transaction(
        &self,
        _transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction> {
        Box::pin(fuel_core_services::stream::pending())
    }

    fn notify_gossip_transaction_validity(
        &self,
        _message_info: GossipsubMessageInfo,
        _validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

impl fuel_core_txpool::ports::TxPoolDb for Database {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>> {
        self.storage::<Coins>()
            .get(utxo_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool> {
        self.storage::<ContractsRawCode>().contains_key(contract_id)
    }

    fn message(
        &self,
        message_id: &MessageId,
    ) -> StorageResult<Option<CompressedMessage>> {
        self.storage::<Messages>()
            .get(message_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn is_message_spent(&self, message_id: &MessageId) -> StorageResult<bool> {
        self.storage::<SpentMessages>().contains_key(message_id)
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }
}
