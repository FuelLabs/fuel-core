use super::MaybeRelayerAdapter;
use crate::{
    database::Database,
    service::adapters::{
        ExecutorAdapter,
        TransactionsSource,
    },
};
use fuel_core_executor::{
    executor::{
        ExecutionBlockWithSource,
        Executor,
    },
    ports::MaybeCheckedTransaction,
};
use fuel_core_storage::{
    transactional::StorageTransaction,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_tx,
    fuel_tx::Receipt,
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
        Nonce,
    },
    services::{
        block_producer::Components,
        executor::{
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::TransactionStatus,
    },
};

impl fuel_core_executor::ports::TransactionsSource for TransactionsSource {
    fn next(&self, gas_limit: u64) -> Vec<MaybeCheckedTransaction> {
        self.txpool
            .select_transactions(gas_limit)
            .into_iter()
            .map(|tx| MaybeCheckedTransaction::CheckedTransaction(tx.as_ref().into()))
            .collect()
    }
}

impl ExecutorAdapter {
    pub(crate) fn _execute_without_commit(
        &self,
        block: ExecutionBlockWithSource<TransactionsSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        let executor = Executor {
            database: self.relayer.database.clone(),
            relayer: self.relayer.clone(),
            config: self.config.clone(),
        };
        executor.execute_without_commit(block, self.config.as_ref().into())
    }

    pub(crate) fn _dry_run(
        &self,
        block: Components<fuel_tx::Transaction>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        let executor = Executor {
            database: self.relayer.database.clone(),
            relayer: self.relayer.clone(),
            config: self.config.clone(),
        };
        executor.dry_run(block, utxo_validation)
    }
}

/// Implemented to satisfy: `GenesisCommitment for ContractRef<&'a mut Database>`
impl fuel_core_executor::refs::ContractStorageTrait for Database {
    type InnerError = StorageError;
}

impl fuel_core_executor::ports::MessageIsSpent for Database {
    type Error = StorageError;

    fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_is_spent(nonce)
    }
}

impl fuel_core_executor::ports::TxIdOwnerRecorder for Database {
    type Error = StorageError;

    fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error> {
        self.record_tx_id_owner(owner, block_height, tx_idx, tx_id)
    }

    fn update_tx_status(
        &self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Self::Error> {
        self.update_tx_status(id, status)
    }
}

impl fuel_core_executor::ports::ExecutorDatabaseTrait<Database> for Database {}

impl fuel_core_executor::ports::RelayerPort for MaybeRelayerAdapter {
    fn get_message(
        &self,
        id: &Nonce,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>> {
        #[cfg(feature = "relayer")]
        {
            match self.relayer_synced.as_ref() {
                Some(sync) => sync.get_message(id, _da_height),
                None => {
                    if *_da_height <= self.da_deploy_height {
                        Ok(fuel_core_storage::StorageAsRef::storage::<
                            fuel_core_storage::tables::Messages,
                        >(&self.database)
                        .get(id)?
                        .map(std::borrow::Cow::into_owned))
                    } else {
                        Ok(None)
                    }
                }
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(fuel_core_storage::StorageAsRef::storage::<
                fuel_core_storage::tables::Messages,
            >(&self.database)
            .get(id)?
            .map(std::borrow::Cow::into_owned))
        }
    }
}

#[cfg(test)]
/// For some tests we don't care about the actual implementation of
/// the RelayerPort and using a passthrough is fine.
impl fuel_core_executor::ports::RelayerPort for Database {
    fn get_message(
        &self,
        id: &Nonce,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsRef,
        };
        use std::borrow::Cow;
        Ok(self.storage::<Messages>().get(id)?.map(Cow::into_owned))
    }
}
