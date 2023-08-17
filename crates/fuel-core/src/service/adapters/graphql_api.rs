use crate::{
    database::{
        transactions::OwnedTransactionIndexCursor,
        Database,
    },
    fuel_core_graphql_api::ports::{
        BlockProducerPort,
        DatabaseBlocks,
        DatabaseChain,
        DatabaseCoins,
        DatabaseContracts,
        DatabaseMessageProof,
        DatabaseMessages,
        DatabasePort,
        DatabaseTransactions,
        DryRunExecution,
        TxPoolPort,
    },
    service::adapters::TxPoolAdapter,
};
use async_trait::async_trait;
use fuel_core_services::stream::{
    BoxFuture,
    BoxStream,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::{
    service::TxStatusMessage,
    types::{
        ContractId,
        TxId,
    },
};
use fuel_core_types::{
    blockchain::primitives::{
        BlockId,
        DaBlockHeight,
    },
    entities::message::{
        MerkleProof,
        Message,
    },
    fuel_tx::{
        Address,
        AssetId,
        Receipt as TxReceipt,
        Transaction,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        Nonce,
    },
    services::{
        graphql_api::ContractBalance,
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

impl DatabaseBlocks for Database {
    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId> {
        self.get_block_id(height)
            .and_then(|height| height.ok_or(not_found!("BlockId")))
    }

    fn blocks_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(BlockHeight, BlockId)>> {
        self.all_block_ids(start, direction)
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn ids_of_latest_block(&self) -> StorageResult<(BlockHeight, BlockId)> {
        Ok(self
            .ids_of_latest_block()
            .transpose()
            .ok_or(not_found!("BlockId"))??)
    }
}

impl DatabaseTransactions for Database {
    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        Ok(self
            .get_tx_status(tx_id)
            .transpose()
            .ok_or(not_found!("TransactionId"))??)
    }

    fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>> {
        let start = start.map(|tx_pointer| OwnedTransactionIndexCursor {
            block_height: tx_pointer.block_height(),
            tx_idx: tx_pointer.tx_index(),
        });
        self.owned_transactions(owner, start, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }
}

impl DatabaseMessages for Database {
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Nonce>> {
        self.owned_message_ids(owner, start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>> {
        self.all_messages(start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }
}

impl DatabaseCoins for Database {
    fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<UtxoId>> {
        self.owned_coins_ids(owner, start_coin, Some(direction))
            .map(|res| res.map_err(StorageError::from))
            .into_boxed()
    }
}

impl DatabaseContracts for Database {
    fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        self.contract_balances(contract, start_asset, Some(direction))
            .map(move |result| {
                result
                    .map_err(StorageError::from)
                    .map(|(asset_id, amount)| ContractBalance {
                        owner: contract,
                        amount,
                        asset_id,
                    })
            })
            .into_boxed()
    }
}

impl DatabaseChain for Database {
    fn chain_name(&self) -> StorageResult<String> {
        pub const DEFAULT_NAME: &str = "Fuel.testnet";

        Ok(self
            .get_chain_name()?
            .unwrap_or_else(|| DEFAULT_NAME.to_string()))
    }

    fn base_chain_height(&self) -> StorageResult<DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            self.get_finalized_da_height()
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(0u64.into())
        }
    }
}

impl DatabasePort for Database {}

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
        self.service.insert(txs).await
    }

    fn tx_update_subscribe(&self, id: TxId) -> BoxFuture<BoxStream<TxStatusMessage>> {
        Box::pin(self.service.tx_update_subscribe(id))
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
impl DryRunExecution for BlockProducerAdapter {
    async fn dry_run_tx(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<TxReceipt>> {
        self.block_producer
            .dry_run(transaction, height, utxo_validation)
            .await
    }
}

impl BlockProducerPort for BlockProducerAdapter {}

use super::BlockProducerAdapter;
