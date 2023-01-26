use crate::{
    database::Database,
    executor::Executor,
    service::adapters::ExecutorAdapter,
};
use fuel_core_executor::refs::ContractStorageTrait;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Error as StorageError,
};
use fuel_core_types::{
    fuel_tx::Receipt,
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult,
    },
};

impl ExecutorAdapter {
    pub(crate) fn _execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        let executor = Executor {
            database: self.database.clone(),
            config: self.config.clone(),
        };
        executor.execute_without_commit(block)
    }

    pub(crate) fn _dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        let executor = Executor {
            database: self.database.clone(),
            config: self.config.clone(),
        };
        executor.dry_run(block, utxo_validation)
    }
}

/// Implemented to satisfy: `GenesisCommitment for ContractRef<&'a mut Database>`
impl ContractStorageTrait for Database {
    type InnerError = StorageError;
}
