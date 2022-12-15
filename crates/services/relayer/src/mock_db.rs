#![allow(missing_docs)]

use async_trait::async_trait;

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_core_storage::{
    tables::Messages,
    Error as StorageError,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
        SealedBlock,
    },
    entities::message::Message,
    fuel_tx::MessageId,
};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default)]
pub struct Data {
    pub messages: HashMap<MessageId, Message>,
    pub chain_height: BlockHeight,
    pub sealed_blocks: HashMap<BlockHeight, Arc<SealedBlock>>,
    pub finalized_da_height: Option<DaBlockHeight>,
    pub last_committed_finalized_fuel_height: BlockHeight,
    pub pending_committed_fuel_height: Option<BlockHeight>,
}

#[derive(Default, Clone)]
/// Type for mocking the database when testing the relayer.
/// Note that this type is clone but internally it is wrapped
/// in an [`Arc`] [`Mutex`] so only the pointer is cloned.
pub struct MockDb {
    pub data: Arc<Mutex<Data>>,
}

impl StorageInspect<Messages> for MockDb {
    type Error = StorageError;

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

#[async_trait]
impl RelayerDb for MockDb {
    async fn get_chain_height(&self) -> BlockHeight {
        self.data.lock().unwrap().chain_height
    }

    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedBlock>> {
        self.data
            .lock()
            .unwrap()
            .sealed_blocks
            .get(&height)
            .cloned()
    }

    async fn set_finalized_da_height(&self, height: DaBlockHeight) {
        self.data.lock().unwrap().finalized_da_height = Some(height);
    }

    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        self.data.lock().unwrap().finalized_da_height
    }

    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight> {
        self.data.lock().unwrap().pending_committed_fuel_height
    }

    async fn set_last_published_fuel_height(&self, block_height: BlockHeight) {
        self.data.lock().unwrap().pending_committed_fuel_height = Some(block_height);
    }
}
