use crate::database::columns::TRANSACTION_STATUS;
use crate::database::{columns::TRANSACTIONS, Database, KvStore, KvStoreError};
use crate::state::{Error, IterDirection};
use crate::tx_pool::TransactionStatus;
use fuel_tx::{Bytes32, Transaction};
use std::ops::Deref;

impl KvStore<Bytes32, Transaction> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        Database::insert(&self, key.as_ref(), TRANSACTIONS, value.clone()).map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::remove(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::get(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
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
        Database::insert(&self, tx_id.to_vec(), TRANSACTION_STATUS, status)
    }

    pub fn get_tx_status(&self, tx_id: &Bytes32) -> Result<Option<TransactionStatus>, Error> {
        Database::get(&self, &tx_id.deref()[..], TRANSACTION_STATUS)
    }
}
