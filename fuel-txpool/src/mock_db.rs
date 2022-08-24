use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_tx::{Contract, ContractId, MessageId, UtxoId},
    },
    db::{Error, KvStoreError},
    model::{Coin, Message},
    txpool::TxPoolDb,
};

#[derive(Default)]
pub(crate) struct Data {
    pub coins: HashMap<UtxoId, Coin>,
    pub contracts: HashMap<ContractId, Contract>,
    pub messages: HashMap<MessageId, Message>,
}

#[derive(Default)]
pub(crate) struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl Storage<UtxoId, Coin> for MockDb {
    type Error = KvStoreError;

    fn insert(&mut self, key: &UtxoId, value: &Coin) -> Result<Option<Coin>, Self::Error> {
        Ok(self.data.lock().unwrap().coins.insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<Coin>, Self::Error> {
        Ok(self.data.lock().unwrap().coins.remove(key))
    }

    fn get(&self, key: &UtxoId) -> Result<Option<Cow<Coin>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .coins
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().coins.contains_key(key))
    }
}

impl Storage<ContractId, Contract> for MockDb {
    type Error = Error;

    fn insert(
        &mut self,
        key: &ContractId,
        value: &Contract,
    ) -> Result<Option<Contract>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .contracts
            .insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Self::Error> {
        Ok(self.data.lock().unwrap().contracts.remove(key))
    }

    fn get(&self, key: &ContractId) -> Result<Option<Cow<Contract>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .contracts
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().contracts.contains_key(key))
    }
}

impl Storage<MessageId, Message> for MockDb {
    type Error = KvStoreError;

    fn insert(&mut self, key: &MessageId, value: &Message) -> Result<Option<Message>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &MessageId) -> Result<Option<Message>, Self::Error> {
        Ok(self.data.lock().unwrap().messages.remove(key))
    }

    fn get(&self, key: &MessageId) -> Result<Option<Cow<Message>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, key: &MessageId) -> Result<bool, Self::Error> {
        Ok(self.data.lock().unwrap().messages.contains_key(key))
    }
}

impl TxPoolDb for MockDb {}
