use fuel_core_storage::{
    StorageAsRef,
    column::Column,
    kv_store::KeyValueInspect,
    tables::{
        Coins,
        ConsensusParametersVersions,
    },
    transactional::StorageTransaction,
};
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        ConsensusParameters,
        UtxoId,
    },
};

use crate::scheduler::SchedulerError;

pub(crate) trait GetCoin {
    type Error;

    fn get_coin(&self, utxo_id: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error>;
}

impl<S> GetCoin for StorageTransaction<S>
where
    S: KeyValueInspect<Column = Column>,
{
    type Error = crate::scheduler::SchedulerError;

    fn get_coin(&self, utxo_id: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error> {
        self.storage_as_ref::<Coins>()
            .get(utxo_id)
            .map_err(Self::Error::StorageError)
            .map(|v| v.map(|coin| coin.into_owned()))
    }
}

pub(crate) trait GetConsensusParameters {
    type Error;

    fn get_consensus_parameters(
        &self,
        version: u32,
    ) -> Result<ConsensusParameters, Self::Error>;
}

impl<S> GetConsensusParameters for StorageTransaction<S>
where
    S: KeyValueInspect<Column = Column>,
{
    type Error = crate::scheduler::SchedulerError;

    fn get_consensus_parameters(
        &self,
        version: u32,
    ) -> Result<ConsensusParameters, Self::Error> {
        self.storage_as_ref::<ConsensusParametersVersions>()
            .get(&version)
            .map_err(Self::Error::StorageError)?
            .ok_or(SchedulerError::InternalError(
                "Consensus parameters not found".to_string(),
            ))
            .map(|v| v.into_owned())
    }
}
