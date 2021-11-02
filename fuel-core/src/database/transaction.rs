use crate::{
    database::{
        columns::{TRANSACTIONS, TRANSACTION_STATUS},
        Database, KvStoreError,
    },
    state::{Error, IterDirection},
    tx_pool::TransactionStatus,
};
use fuel_storage::Storage;
use fuel_tx::{Bytes32, Transaction};
use std::borrow::Cow;
use std::ops::Deref;

impl Storage<Bytes32, Transaction> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        Database::insert(self, key.as_ref(), TRANSACTIONS, value.clone()).map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::remove(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Transaction>>, KvStoreError> {
        Database::get(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }
}

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Transaction, Error>> + '_ {
        let start = start.map(|b| b.as_ref().to_vec());
        self.iter_all::<Vec<u8>, Transaction>(TRANSACTIONS, None, start, direction)
            .map(|res| res.map(|(_, tx)| tx))
    }

    pub fn update_tx_status(
        &self,
        tx_id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Error> {
        self.insert(tx_id.to_vec(), TRANSACTION_STATUS, status)
    }

    pub fn get_tx_status(&self, tx_id: &Bytes32) -> Result<Option<TransactionStatus>, Error> {
        self.get(&tx_id.deref()[..], TRANSACTION_STATUS)
    }
}
