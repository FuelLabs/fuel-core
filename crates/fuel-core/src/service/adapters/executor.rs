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
    entities::message::CompressedMessage,
    fuel_tx::Receipt,
    fuel_types::MessageId,
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
        id: &MessageId,
        da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<CompressedMessage>> {
        #[cfg(feature = "relayer")]
        {
            match self.relayer_synced.as_ref() {
                Some(sync) => sync.get_message(id, da_height),
                None => {
                    if *da_height <= self.da_deploy_height {
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
            let _ = id;
            let _ = da_height;
            Ok(None)
        }
    }
}
