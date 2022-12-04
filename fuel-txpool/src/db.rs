use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_tx::{
            ContractId,
            MessageId,
            UtxoId,
        },
    },
    model::{
        BlockHeight,
        Coin,
        Message,
    },
};
use fuel_database::{
    tables::{
        Coins,
        ContractsRawCode,
        Messages,
    },
    Database,
    Error,
    KvStoreError,
};

pub trait TxPoolDb: Sync + Send {
    fn utxo(&self, utxo_id: &UtxoId) -> Result<Option<Coin>, KvStoreError>;

    fn contract_exist(&self, contract_id: &ContractId) -> Result<bool, Error>;

    fn message(&self, message_id: &MessageId) -> Result<Option<Message>, KvStoreError>;

    fn current_block_height(&self) -> Result<BlockHeight, KvStoreError>;
}

impl TxPoolDb for Database {
    fn utxo(&self, utxo_id: &UtxoId) -> Result<Option<Coin>, KvStoreError> {
        self.storage::<Coins>()
            .get(utxo_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn contract_exist(&self, contract_id: &ContractId) -> Result<bool, Error> {
        self.storage::<ContractsRawCode>().contains_key(contract_id)
    }

    fn message(&self, message_id: &MessageId) -> Result<Option<Message>, KvStoreError> {
        self.storage::<Messages>()
            .get(message_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn current_block_height(&self) -> Result<BlockHeight, KvStoreError> {
        self.get_block_height()
            .map(|h| h.unwrap_or_default())
            .map_err(Into::into)
    }
}
