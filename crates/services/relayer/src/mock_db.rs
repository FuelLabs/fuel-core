#![allow(missing_docs)]

use crate::ports::RelayerDb;
use fuel_core_storage::{
    not_found,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_types::Nonce,
    services::relayer::Event,
};
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub messages: BTreeMap<DaBlockHeight, HashMap<Nonce, Message>>,
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
    pub fn get_message(&self, id: &Nonce) -> Option<Message> {
        self.data
            .lock()
            .unwrap()
            .messages
            .iter()
            .find_map(|(_, map)| map.get(id).cloned())
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
                        .insert(*message.id(), message.clone());
                }
            }
        }
        let max = m.finalized_da_height.get_or_insert(0u64.into());
        *max = (*max).max(*da_height);
        Ok(())
    }

    fn set_finalized_da_height_to_at_least(
        &mut self,
        height: &DaBlockHeight,
    ) -> StorageResult<()> {
        let mut lock = self.data.lock().unwrap();
        let max = lock.finalized_da_height.get_or_insert(0u64.into());
        *max = (*max).max(*height);
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        self.data
            .lock()
            .unwrap()
            .finalized_da_height
            .ok_or(not_found!("FinalizedDaHeight for test"))
    }
}
