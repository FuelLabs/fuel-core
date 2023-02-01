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
    blockchain::primitives::DaBlockHeight,
    fuel_tx::Receipt,
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult,
    },
};

use super::MaybeRelayerAdapter;

impl ExecutorAdapter {
    pub(crate) fn _execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        let executor = Executor {
            database: self.relayer.database.clone(),
            relayer: self.relayer.clone(),
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
            database: self.relayer.database.clone(),
            relayer: self.relayer.clone(),
            config: self.config.clone(),
        };
        executor.dry_run(block, utxo_validation)
    }
}

/// Implemented to satisfy: `GenesisCommitment for ContractRef<&'a mut Database>`
impl ContractStorageTrait for Database {
    type InnerError = StorageError;
}

impl crate::executor::RelayerPort for MaybeRelayerAdapter {
    fn get_message(
        &self,
        id: &fuel_core_types::fuel_types::MessageId,
        da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<fuel_core_types::entities::message::Message>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsRef,
        };
        use std::borrow::Cow;
        #[cfg(feature = "relayer")]
        {
            match self.relayer_synced.as_ref() {
                Some(sync) => sync.get_message(id, da_height),
                None => Ok(self
                    .database
                    .storage::<Messages>()
                    .get(id)?
                    .map(Cow::into_owned)),
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(self
                .database
                .storage::<Messages>()
                .get(&id)?
                .map(Cow::into_owned))
        }
    }
}
