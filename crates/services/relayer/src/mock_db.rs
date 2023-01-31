#![allow(missing_docs)]

use crate::ports::RelayerDb;
use fuel_core_storage::{
    not_found,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::{
        CheckedMessage,
        Message,
    },
    fuel_tx::MessageId,
};
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    ops::RangeInclusive,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub messages: BTreeMap<DaBlockHeight, HashMap<MessageId, Message>>,
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
    pub fn get_message(&self, id: &MessageId) -> Option<Message> {
        self.data
            .lock()
            .unwrap()
            .messages
            .iter()
            .find_map(|(_, map)| map.get(id).cloned())
    }
}

impl RelayerDb for MockDb {
    fn insert_messages(
        &mut self,
        messages: &[(DaBlockHeight, CheckedMessage)],
    ) -> StorageResult<()> {
        let mut m = self.data.lock().unwrap();
        for (height, message) in messages {
            let (message_id, message) = message.clone().unpack();
            m.messages
                .entry(*height)
                .or_default()
                .insert(message_id, message);
            let max = m.finalized_da_height.get_or_insert(0u64.into());
            *max = (*max).max(*height);
        }
        Ok(())
    }

    fn set_finalized_da_height(&mut self, height: DaBlockHeight) -> StorageResult<()> {
        self.data.lock().unwrap().finalized_da_height = Some(height);
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        self.data
            .lock()
            .unwrap()
            .finalized_da_height
            .ok_or(not_found!("FinalizedDaHeight for test"))
    }

    fn get_opaque_messages(
        &self,
        range: RangeInclusive<DaBlockHeight>,
    ) -> Box<dyn Iterator<Item = &[u8]>> {
        todo!()
    }
}
