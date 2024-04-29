#![allow(missing_docs)]

use crate::ports::RelayerDb;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        relayer::transaction::RelayedTransactionId,
        Message,
        RelayedTransaction,
    },
    fuel_types::Nonce,
    services::relayer::Event,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub messages: BTreeMap<DaBlockHeight, Vec<(Nonce, Message)>>,
    pub transactions:
        BTreeMap<DaBlockHeight, Vec<(RelayedTransactionId, RelayedTransaction)>>,
    pub finalized_da_height: Option<DaBlockHeight>,
}

// TODO: Maybe remove `Arc<Mutex<>>`
#[derive(Default, Clone)]
/// Type for mocking the database when testing the relayer.
/// Note that this type is clone but internally it is wrapped
/// in an [`Arc`] [`Mutex`] so only the pointer is cloned.
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl MockDb {
    pub fn get_message(&self, nonce: &Nonce) -> Option<Message> {
        self.data
            .lock()
            .unwrap()
            .messages
            .iter()
            .find_map(|(_, map)| {
                map.iter()
                    .find(|(inner_nonce, _msg)| nonce == inner_nonce)
                    .map(|(_, msg)| msg.clone())
            })
    }

    pub fn get_messages_for_block(&self, da_block_height: DaBlockHeight) -> Vec<Message> {
        self.data
            .lock()
            .unwrap()
            .messages
            .get(&da_block_height)
            .map(|map| map.iter().map(|(_, msg)| msg).cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_transaction(
        &self,
        id: &RelayedTransactionId,
    ) -> Option<RelayedTransaction> {
        self.data
            .lock()
            .unwrap()
            .transactions
            .iter()
            .find_map(|(_, txs)| {
                txs.iter()
                    .find(|(inner_id, _tx)| id == inner_id)
                    .map(|(_, tx)| tx.clone())
            })
    }

    pub fn get_transactions_for_block(
        &self,
        da_block_height: DaBlockHeight,
    ) -> Vec<RelayedTransaction> {
        self.data
            .lock()
            .unwrap()
            .transactions
            .get(&da_block_height)
            .map(|map| map.iter().map(|(_, tx)| tx).cloned().collect())
            .unwrap_or_default()
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_finalized_da_height_to_at_least(
        &mut self,
        height: &DaBlockHeight,
    ) -> StorageResult<()> {
        let mut lock = self.data.lock().unwrap();
        let max = lock.finalized_da_height.get_or_insert(0u64.into());
        *max = (*max).max(*height);
        Ok(())
    }
}

impl RelayerDb for MockDb {
    fn insert_events(
        &mut self,
        da_height: &DaBlockHeight,
        events: &[Event],
    ) -> StorageResult<()> {
        let mut m = self.data.lock().unwrap();
        for event in events {
            match event {
                Event::Message(message) => {
                    m.messages
                        .entry(message.da_height())
                        .or_default()
                        .push((*message.id(), message.clone()));
                }
                Event::Transaction(transaction) => {
                    m.transactions
                        .entry(transaction.da_height())
                        .or_default()
                        .push((transaction.id(), transaction.clone()));
                }
            }
        }
        let max = m.finalized_da_height.get_or_insert(0u64.into());
        *max = (*max).max(*da_height);
        Ok(())
    }

    fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        self.data.lock().unwrap().finalized_da_height
    }
}
