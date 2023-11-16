use fuel_core_storage::transactional::Transaction;
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_tx::ContractId,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    tai64::Tai64,
};
use std::ops::DerefMut;

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D>;

    fn transaction(&self) -> Self::T;
}

pub trait FuelBlockTrait {
    type Error;

    fn latest_height(&self) -> Result<BlockHeight, Self::Error>;
    fn block_time(&self, height: &BlockHeight) -> Result<Tai64, Self::Error>;
    fn get_block_id(&self, height: &BlockHeight) -> Result<Option<BlockId>, Self::Error>;
}

pub trait FuelStateTrait {
    type Error;

    fn init_contract_state<S: Iterator<Item = (Bytes32, Bytes32)>>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), Self::Error>;
}