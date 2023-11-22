use fuel_core_storage::transactional::Transaction;
use fuel_core_types::{
    blockchain::{
        header::ConsensusHeader,
        primitives::BlockId,
    },
    fuel_tx::{
        Address,
        ContractId,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::txpool::TransactionStatus,
    tai64::Tai64,
};
use std::ops::DerefMut;

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D> + 'static;

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

pub struct ExecutorVmDatabase<D> {
    pub current_block_height: BlockHeight,
    pub current_timestamp: Tai64,
    pub coinbase: ContractId,
    pub database: D,
}

impl<D> ExecutorVmDatabase<D> {
    pub fn new<T>(
        database: D,
        header: &ConsensusHeader<T>,
        coinbase: ContractId,
    ) -> Self {
        Self {
            current_block_height: header.height,
            current_timestamp: header.time,
            coinbase,
            database,
        }
    }
}
