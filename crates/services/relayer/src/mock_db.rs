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
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub messages: HashMap<MessageId, Message>,
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
        self.data.lock().unwrap().messages.get(id).map(Clone::clone)
    }
}

impl RelayerDb for MockDb {
    fn insert_message(
        &mut self,
        message: &CheckedMessage,
    ) -> StorageResult<Option<Message>> {
        let (message_id, message) = message.clone().unpack();
        Ok(self
            .data
            .lock()
            .unwrap()
            .messages
            .insert(message_id, message))
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
}
