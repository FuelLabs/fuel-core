use crate::ports::TxPoolDb;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::{
        coin::Coin,
        message::Message,
    },
    fuel_tx::{
        Contract,
        ContractId,
        MessageId,
        UtxoId,
    },
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub coins: HashMap<UtxoId, Coin>,
    pub contracts: HashMap<ContractId, Contract>,
    pub messages: HashMap<MessageId, Message>,
}

#[derive(Clone, Default)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
    pub fn insert_coin(&self, id: UtxoId, coin: Coin) {
        self.data.lock().unwrap().coins.insert(id, coin);
    }

    pub fn insert_message(&self, message: Message) {
        self.data
            .lock()
            .unwrap()
            .messages
            .insert(message.id(), message);
    }
}

impl TxPoolDb for MockDb {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<Coin>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .coins
            .get(utxo_id)
            .map(Clone::clone))
    }

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .contracts
            .contains_key(contract_id))
    }

    fn message(&self, message_id: &MessageId) -> StorageResult<Option<Message>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .get(message_id)
            .map(Clone::clone))
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        Ok(Default::default())
    }
}
