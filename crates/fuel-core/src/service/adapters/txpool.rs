use crate::{
    database::OnChainIterableKeyValueView,
    service::adapters::{
        BlockImporterAdapter,
        ConsensusParametersProvider,
        P2PAdapter,
        StaticGasPrice,
    },
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsRawCode,
        Messages,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::ports::{
    BlockImporter,
    ConsensusParametersProvider as ConsensusParametersProviderTrait,
    GasPriceProvider,
};
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        BlobId,
        ConsensusParameters,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        Nonce,
    },
    fuel_vm::BlobData,
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            PeerId,
            TransactionGossipData,
        },
    },
};
use std::sync::Arc;

impl BlockImporter for BlockImporterAdapter {
    fn block_events(&self) -> BoxStream<SharedImportResult> {
        self.events_shared_result()
    }
}

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl fuel_core_txpool::ports::NotifyP2P for P2PAdapter {
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()> {
        if let Some(service) = &self.service {
            service.broadcast_transaction(transaction)
        } else {
            Ok(())
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

#[cfg(feature = "p2p")]
impl fuel_core_txpool::ports::P2PSubscriptions for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

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

    fn subscribe_new_peers(&self) -> BoxStream<PeerId> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        if let Some(service) = &self.service {
            Box::pin(
                BroadcastStream::new(service.subscribe_new_peers())
                    .filter_map(|result| result.ok()),
            )
        } else {
            Box::pin(fuel_core_services::stream::pending())
        }
    }
}

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl fuel_core_txpool::ports::P2PRequests for P2PAdapter {
    async fn request_tx_ids(&self, peer_id: PeerId) -> anyhow::Result<Vec<TxId>> {
        if let Some(service) = &self.service {
            service.get_all_transactions_ids_from_peer(peer_id).await
        } else {
            Ok(vec![])
        }
    }

    async fn request_txs(
        &self,
        peer_id: PeerId,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<Transaction>>> {
        if let Some(service) = &self.service {
            service
                .get_full_transactions_from_peer(peer_id, tx_ids)
                .await
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(not(feature = "p2p"))]
const _: () = {
    #[async_trait::async_trait]
    impl fuel_core_txpool::ports::NotifyP2P for P2PAdapter {
        fn broadcast_transaction(
            &self,
            _transaction: Arc<Transaction>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn notify_gossip_transaction_validity(
            &self,
            _message_info: GossipsubMessageInfo,
            _validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl fuel_core_txpool::ports::P2PSubscriptions for P2PAdapter {
        type GossipedTransaction = TransactionGossipData;

        fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction> {
            Box::pin(fuel_core_services::stream::pending())
        }

        fn subscribe_new_peers(&self) -> BoxStream<PeerId> {
            Box::pin(fuel_core_services::stream::pending())
        }
    }

    #[async_trait::async_trait]
    impl fuel_core_txpool::ports::P2PRequests for P2PAdapter {
        async fn request_tx_ids(&self, _peer_id: PeerId) -> anyhow::Result<Vec<TxId>> {
            Ok(vec![])
        }

        async fn request_txs(
            &self,
            _peer_id: PeerId,
            _tx_ids: Vec<TxId>,
        ) -> anyhow::Result<Vec<Option<Transaction>>> {
            Ok(vec![])
        }
    }
};

impl fuel_core_txpool::ports::TxPoolPersistentStorage for OnChainIterableKeyValueView {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>> {
        self.storage::<Coins>()
            .get(utxo_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool> {
        self.storage::<ContractsRawCode>().contains_key(contract_id)
    }

    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool> {
        self.storage::<BlobData>().contains_key(blob_id)
    }

    fn message(&self, id: &Nonce) -> StorageResult<Option<Message>> {
        self.storage::<Messages>()
            .get(id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }
}

#[async_trait::async_trait]
impl GasPriceProvider for StaticGasPrice {
    fn next_gas_price(&self) -> u64 {
        self.gas_price
    }
}

impl ConsensusParametersProviderTrait for ConsensusParametersProvider {
    fn latest_consensus_parameters(
        &self,
    ) -> (ConsensusParametersVersion, Arc<ConsensusParameters>) {
        self.shared_state.latest_consensus_parameters_with_version()
    }
}
