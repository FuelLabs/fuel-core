use crate::{
    database::{
        database_description::relayer::Relayer,
        Database,
    },
    service::adapters::{
        ExecutorAdapter,
        TransactionsSource,
    },
};
use fuel_core_executor::{
    executor::ExecutionBlockWithSource,
    ports::MaybeCheckedTransaction,
};
use fuel_core_storage::{
    transactional::StorageTransaction,
    Error as StorageError,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx,
    services::{
        block_producer::Components,
        executor::{
            Result as ExecutorResult,
            TransactionExecutionStatus,
            UncommittedResult,
        },
        relayer::Event,
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
    pub(crate) fn _execute_without_commit<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>>
    where
        TxSource: fuel_core_executor::ports::TransactionsSource,
    {
        self.executor.execute_without_commit(block)
    }

    pub(crate) fn _dry_run(
        &self,
        block: Components<Vec<fuel_tx::Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        self.executor.dry_run(block, utxo_validation)
    }
}

/// Implemented to satisfy: `GenesisCommitment for ContractRef<&'a mut Database>`
impl fuel_core_executor::refs::ContractStorageTrait for Database {
    type InnerError = StorageError;
}

impl fuel_core_executor::ports::ExecutorDatabaseTrait<Database> for Database {}

impl fuel_core_executor::ports::RelayerPort for Database<Relayer> {
    fn enabled(&self) -> bool {
        #[cfg(feature = "relayer")]
        {
            true
        }
        #[cfg(not(feature = "relayer"))]
        {
            false
        }
    }

    fn get_events(&self, _da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_storage::StorageAsRef;
            let events = self
                .storage::<fuel_core_relayer::storage::EventsHistory>()
                .get(_da_height)?
                .map(|cow| cow.into_owned())
                .unwrap_or_default();
            Ok(events)
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(vec![])
        }
    }
}
