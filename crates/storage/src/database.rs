use crate::{
    iter::IterDirection,
    tables::{
        Messages,
        SpentMessages,
    },
    Error as StorageError,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::{
        header::ConsensusHeader,
        primitives::BlockId,
    },
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
        Nonce,
    },
    services::txpool::TransactionStatus,
    tai64::Tai64,
};
use fuel_vm_private::{
    fuel_types::ContractId,
    prelude::InterpreterStorage,
};
use serde::de::DeserializeOwned;

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

pub trait MessageIsSpent:
    StorageInspect<SpentMessages, Error = StorageError>
    + StorageInspect<Messages, Error = StorageError>
{
    type Error;

    fn message_is_spent(&self, nonce: &Nonce) -> Result<bool, StorageError>;
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

pub trait DatabaseColumnIterator {
    type Error;

    fn iter_all_filtered_column<K, V, P, S>(
        &self,
        prefix: Option<P>,
        start: Option<S>,
        direction: Option<IterDirection>,
    ) -> Box<dyn Iterator<Item = Result<(K, V), Self::Error>> + '_>
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
        P: AsRef<[u8]>,
        S: AsRef<[u8]>;
}

pub trait VmDatabaseTrait {
    type Data: InterpreterStorage;

    fn new<T>(&self, header: &ConsensusHeader<T>, coinbase: ContractId) -> Self::Data;
}
