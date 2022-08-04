use crate::{
    database::{columns, Database, KvStoreError},
    state::{Error, IterDirection},
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_types::{Address, Bytes32, MessageId},
    },
    model::DaMessage,
};
use std::borrow::Cow;

impl Storage<MessageId, DaMessage> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &MessageId,
        value: &DaMessage,
    ) -> Result<Option<DaMessage>, KvStoreError> {
        // insert secondary record by owner
        Database::insert(
            self,
            owner_msg_id_key(&value.owner, key),
            columns::OWNED_MESSAGE_IDS,
            true,
        )?;

        // insert primary record
        Database::insert(self, key.as_ref(), columns::DA_MESSAGES, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &MessageId) -> Result<Option<DaMessage>, KvStoreError> {
        let result: Option<DaMessage> = Database::remove(self, key.as_ref(), columns::DA_MESSAGES)?;

        if let Some(da_msg) = &result {
            Database::remove::<bool>(
                self,
                &owner_msg_id_key(&da_msg.owner, key),
                columns::OWNED_MESSAGE_IDS,
            )?;
        }

        Ok(result)
    }

    fn get(&self, key: &MessageId) -> Result<Option<Cow<DaMessage>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }

    fn contains_key(&self, key: &MessageId) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }
}

impl Database {
    pub fn owned_message_ids(
        &self,
        owner: Address,
        start_message_id: Option<MessageId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<MessageId, Error>> + '_ {
        self.iter_all::<Vec<u8>, bool>(
            columns::OWNED_MESSAGE_IDS,
            Some(owner.as_ref().to_vec()),
            start_message_id.map(|msg_id| owner_msg_id_key(&owner, &msg_id)),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| {
            res.map(|(key, _)| {
                MessageId::new(unsafe { *Bytes32::from_slice_unchecked(&key[32..64]) })
            })
        })
    }
}

/// Get a Key by chaining Owner + MessageId
fn owner_msg_id_key(owner: &Address, msg_id: &MessageId) -> Vec<u8> {
    owner
        .as_ref()
        .iter()
        .chain(msg_id.as_ref().iter())
        .copied()
        .collect()
}
