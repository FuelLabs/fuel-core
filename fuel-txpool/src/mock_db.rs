use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            Contract,
            ContractId,
            MessageId,
            UtxoId,
        },
    },
    db::{
        Coins,
        ContractsRawCode,
        Error,
        KvStoreError,
        Messages,
    },
    model::{
        Coin,
        Message,
    },
    txpool::TxPoolDb,
};

#[derive(Default)]
pub struct Data {
    pub coins: HashMap<UtxoId, Coin>,
    pub contracts: HashMap<ContractId, Contract>,
    pub messages: HashMap<MessageId, Message>,
}

#[derive(Default)]
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

// TODO: Generate storage implementation with macro.

impl StorageInspect<Coins> for MockDb {
    type Error = KvStoreError;

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

impl StorageMutate<Coins> for MockDb {
    fn insert(
        &mut self,
        key: &UtxoId,
        value: &Coin,
    ) -> Result<Option<Coin>, Self::Error> {
        Ok(self.data.lock().unwrap().coins.insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<Coin>, Self::Error> {
        Ok(self.data.lock().unwrap().coins.remove(key))
    }
}

impl StorageInspect<ContractsRawCode> for MockDb {
    type Error = Error;

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

impl StorageMutate<ContractsRawCode> for MockDb {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &[u8],
    ) -> Result<Option<Contract>, Self::Error> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .contracts
            .insert(*key, value.into()))
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Self::Error> {
        Ok(self.data.lock().unwrap().contracts.remove(key))
    }
}

impl StorageInspect<Messages> for MockDb {
    type Error = KvStoreError;

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

impl StorageMutate<Messages> for MockDb {
    fn insert(
        &mut self,
        key: &MessageId,
        value: &Message,
    ) -> Result<Option<Message>, Self::Error> {
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
}

impl TxPoolDb for MockDb {}
