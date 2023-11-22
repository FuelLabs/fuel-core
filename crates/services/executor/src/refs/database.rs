use fuel_core_storage::transactional::{Transaction};
use fuel_core_types::{
    fuel_tx::{
        Address,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::txpool::TransactionStatus,
};
use std::ops::DerefMut;

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D> + 'static;

    fn transaction(&self) -> Self::T;
}



pub trait TxIdOwnerRecorder {
    type Error;

    fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error>;

    fn update_tx_status(
        &self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Self::Error>;
}
