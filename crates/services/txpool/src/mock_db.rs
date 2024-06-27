use crate::ports::TxPoolDb;
use fuel_core_storage::{
    transactional::AtomicView,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::{
        coins::coin::{
            Coin,
            CompressedCoin,
        },
        relayer::message::Message,
    },
    fuel_tx::{
        Contract,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
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
        Ok(self.data.lock().unwrap().coins.get(utxo_id).cloned())
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
        Ok(self.data.lock().unwrap().messages.get(id).cloned())
    }
}

pub struct MockDBProvider(pub MockDb);

impl AtomicView for MockDBProvider {
    type LatestView = MockDb;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.0.clone())
    }
}
