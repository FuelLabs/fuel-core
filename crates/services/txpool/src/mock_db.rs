use crate::ports::TxPoolDb;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::{
        coins::coin::{
            Coin,
            CompressedCoin,
        },
        message::Message,
    },
    fuel_tx::{
        Contract,
        ContractId,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        Nonce,
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
    pub messages: HashMap<Nonce, Message>,
    pub spent_messages: HashSet<Nonce>,
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

    pub fn insert_message(&self, message: Message) {
        self.data
            .lock()
            .unwrap()
            .messages
            .insert(*message.id(), message);
    }

    pub fn spend_message(&self, id: Nonce) {
        self.data.lock().unwrap().spent_messages.insert(id);
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

    fn message(&self, id: &Nonce) -> StorageResult<Option<Message>> {
        Ok(self.data.lock().unwrap().messages.get(id).map(Clone::clone))
    }

    fn is_message_spent(&self, id: &Nonce) -> StorageResult<bool> {
        Ok(self.data.lock().unwrap().spent_messages.contains(id))
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        Ok(Default::default())
    }

    fn transaction_status(
        &self,
        _tx_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<fuel_core_types::services::txpool::TransactionStatus> {
        unimplemented!()
    }
}
