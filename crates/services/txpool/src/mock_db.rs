use crate::ports::TxPoolDb;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::{
        coin::{
            Coin,
            CompressedCoin,
        },
        message::CompressedMessage,
    },
    fuel_tx::{
        Contract,
        ContractId,
        MessageId,
        UtxoId,
    },
};
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub coins: HashMap<UtxoId, CompressedCoin>,
    pub contracts: HashMap<ContractId, Contract>,
    pub messages: HashMap<MessageId, CompressedMessage>,
    pub spent_messages: HashSet<MessageId>,
}

#[derive(Clone, Default)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
    pub fn insert_coin(&self, coin: Coin) {
        self.data
            .lock()
            .unwrap()
            .coins
            .insert(coin.utxo_id, coin.compress());
    }

    pub fn insert_message(&self, message: CompressedMessage) {
        self.data
            .lock()
            .unwrap()
            .messages
            .insert(message.id(), message);
    }

    pub fn spend_message(&self, message_id: MessageId) {
        self.data.lock().unwrap().spent_messages.insert(message_id);
    }
}

impl TxPoolDb for MockDb {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>> {
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

    fn message(
        &self,
        message_id: &MessageId,
    ) -> StorageResult<Option<CompressedMessage>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .get(message_id)
            .map(Clone::clone))
    }

    fn is_message_spent(&self, message_id: &MessageId) -> StorageResult<bool> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .spent_messages
            .contains(message_id))
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        Ok(Default::default())
    }
}
