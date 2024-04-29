use crate::types::GasPrice;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        ConsensusParameters,
        Transaction,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        ContractId,
        Nonce,
    },
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            NetworkData,
        },
    },
};
use std::{
    ops::Deref,
    sync::Arc,
};

pub trait PeerToPeer: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;

    // Gossip broadcast a transaction inserted via API.
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

    /// Creates a stream of next transactions gossiped from the network.
    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction>;

    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;
}

pub trait BlockImporter: Send + Sync {
    /// Wait until the next block is available
    fn block_events(&self) -> BoxStream<SharedImportResult>;
}

pub trait TxPoolDb: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

/// Trait for getting gas price for the Tx Pool code to look up the gas price for a given block height
pub trait GasPriceProvider {
    /// Get gas price for specific block height if it is known
    fn gas_price(&self, block_height: BlockHeight) -> Option<GasPrice>;
}

/// Trait for getting the latest consensus parameters.
#[cfg_attr(feature = "test-helpers", mockall::automock)]
pub trait ConsensusParametersProvider {
    /// Get latest consensus parameters.
    fn latest_consensus_parameters(&self) -> Arc<ConsensusParameters>;
}

impl<T: GasPriceProvider> GasPriceProvider for Arc<T> {
    fn gas_price(&self, block_height: BlockHeight) -> Option<GasPrice> {
        self.deref().gas_price(block_height)
    }
}
