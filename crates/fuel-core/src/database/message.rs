use crate::database::{
    Column,
    Database,
};
use fuel_core_chain_config::MessageConfig;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        manual::Manual,
        postcard::Postcard,
        Decode,
        Encode,
    },
    iter::IterDirection,
    structured_storage::TableWithBlueprint,
    tables::{
        Messages,
        SpentMessages,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    entities::message::Message,
    fuel_types::{
        Address,
        Nonce,
    },
};
use std::borrow::Cow;

fuel_core_types::fuel_vm::double_key!(OwnedMessageKey, Address, address, Nonce, nonce);

/// The table that stores all messages per owner.
pub struct OwnedMessageIds;

impl Mappable for OwnedMessageIds {
    type Key = OwnedMessageKey;
    type OwnedKey = Self::Key;
    type Value = ();
    type OwnedValue = Self::Value;
}

impl Encode<OwnedMessageKey> for Manual<OwnedMessageKey> {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &OwnedMessageKey) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl Decode<OwnedMessageKey> for Manual<OwnedMessageKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<OwnedMessageKey> {
        OwnedMessageKey::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}

impl TableWithBlueprint for OwnedMessageIds {
    type Blueprint = Plain<Manual<OwnedMessageKey>, Postcard>;

    fn column() -> fuel_core_storage::column::Column {
        Column::OwnedMessageIds
    }
}

impl StorageInspect<Messages> for Database {
    type Error = StorageError;

    fn get(&self, key: &Nonce) -> Result<Option<Cow<Message>>, Self::Error> {
        self.data.storage::<Messages>().get(key)
    }

    fn contains_key(&self, key: &Nonce) -> Result<bool, Self::Error> {
        self.data.storage::<Messages>().contains_key(key)
    }
}

impl StorageMutate<Messages> for Database {
    fn insert(
        &mut self,
        key: &Nonce,
        value: &Message,
    ) -> Result<Option<Message>, Self::Error> {
        // insert primary record
        let result = self.data.storage_as_mut::<Messages>().insert(key, value)?;

        // insert secondary record by owner
        self.storage_as_mut::<OwnedMessageIds>()
            .insert(&OwnedMessageKey::new(value.recipient(), key), &())?;

        Ok(result)
    }

    fn remove(&mut self, key: &Nonce) -> Result<Option<Message>, Self::Error> {
        let result: Option<Message> =
            self.data.storage_as_mut::<Messages>().remove(key)?;

        if let Some(message) = &result {
            self.storage_as_mut::<OwnedMessageIds>()
                .remove(&OwnedMessageKey::new(message.recipient(), key))?;
        }

        Ok(result)
    }
}

impl Database {
    pub fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<Nonce>> + '_ {
        let start_message_id =
            start_message_id.map(|msg_id| OwnedMessageKey::new(owner, &msg_id));
        self.iter_all_filtered::<OwnedMessageIds, _>(
            Some(*owner),
            start_message_id.as_ref(),
            direction,
        )
        .map(|res| res.map(|(key, _)| *key.nonce()))
    }

    pub fn all_messages(
        &self,
        start: Option<Nonce>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<Message>> + '_ {
        self.iter_all_by_start::<Messages>(start.as_ref(), direction)
            .map(|res| res.map(|(_, message)| message))
    }

    pub fn get_message_config(&self) -> StorageResult<Option<Vec<MessageConfig>>> {
        let configs = self
            .all_messages(None, None)
            .filter_map(|msg| {
                // Return only unspent messages
                if let Ok(msg) = msg {
                    match self.message_is_spent(msg.id()) {
                        Ok(false) => Some(Ok(msg)),
                        Ok(true) => None,
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    Some(msg.map_err(StorageError::from))
                }
            })
            .map(|msg| -> StorageResult<MessageConfig> {
                let msg = msg?;

                Ok(MessageConfig {
                    sender: *msg.sender(),
                    recipient: *msg.recipient(),
                    nonce: *msg.nonce(),
                    amount: msg.amount(),
                    data: msg.data().clone(),
                    da_height: msg.da_height(),
                })
            })
            .collect::<StorageResult<Vec<MessageConfig>>>()?;

        Ok(Some(configs))
    }

    pub fn message_is_spent(&self, id: &Nonce) -> StorageResult<bool> {
        fuel_core_storage::StorageAsRef::storage::<SpentMessages>(&self).contains_key(id)
    }

    pub fn message_exists(&self, id: &Nonce) -> StorageResult<bool> {
        fuel_core_storage::StorageAsRef::storage::<Messages>(&self).contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_message_ids() {
        let mut db = Database::default();
        let message = Message::default();

        // insert a message with the first id
        let first_id = 1.into();
        let _ = db
            .storage_as_mut::<Messages>()
            .insert(&first_id, &message)
            .unwrap();

        // insert a message with the second id with the same Owner
        let second_id = 2.into();
        let _ = db
            .storage_as_mut::<Messages>()
            .insert(&second_id, &message)
            .unwrap();

        // verify that 2 message IDs are associated with a single Owner/Recipient
        let owned_msg_ids = db.owned_message_ids(message.recipient(), None, None);
        assert_eq!(owned_msg_ids.count(), 2);

        // remove the first message with its given id
        let _ = db.storage_as_mut::<Messages>().remove(&first_id).unwrap();

        // verify that only second ID is left
        let owned_msg_ids: Vec<_> = db
            .owned_message_ids(message.recipient(), None, None)
            .collect();
        assert_eq!(owned_msg_ids.first().unwrap().as_ref().unwrap(), &second_id);
        assert_eq!(owned_msg_ids.len(), 1);

        // remove the second message with its given id
        let _ = db.storage_as_mut::<Messages>().remove(&second_id).unwrap();
        let owned_msg_ids = db.owned_message_ids(message.recipient(), None, None);
        assert_eq!(owned_msg_ids.count(), 0);
    }
}
