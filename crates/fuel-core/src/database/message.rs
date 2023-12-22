use crate::database::{
    storage::ToDatabaseKey,
    Column,
    Database,
    Result as DatabaseResult,
};
use fuel_core_chain_config::MessageConfig;
use fuel_core_storage::{
    iter::IterDirection,
    tables::{
        Messages,
        SpentMessages,
    },
    Error as StorageError,
    Result as StorageResult,
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
use std::{
    borrow::Cow,
    ops::Deref,
};

use super::storage::DatabaseColumn;

impl StorageInspect<Messages> for Database {
    type Error = StorageError;

    fn get(&self, key: &Nonce) -> Result<Option<Cow<Message>>, Self::Error> {
        let key = key.database_key();
        Database::get(self, key.as_ref(), Column::Messages).map_err(Into::into)
    }

    fn contains_key(&self, key: &Nonce) -> Result<bool, Self::Error> {
        let key = key.database_key();
        Database::contains_key(self, key.as_ref(), Column::Messages).map_err(Into::into)
    }
}

impl StorageMutate<Messages> for Database {
    fn insert(
        &mut self,
        key: &Nonce,
        value: &Message,
    ) -> Result<Option<Message>, Self::Error> {
        // insert primary record
        let result =
            Database::insert(self, key.database_key().as_ref(), Column::Messages, value)?;

        // insert secondary record by owner
        let _: Option<bool> = Database::insert(
            self,
            owner_msg_id_key(&value.recipient, key),
            Column::OwnedMessageIds,
            &true,
        )?;

        Ok(result)
    }

    fn remove(&mut self, key: &Nonce) -> Result<Option<Message>, Self::Error> {
        let result: Option<Message> =
            Database::take(self, key.database_key().as_ref(), Column::Messages)?;

        if let Some(message) = &result {
            Database::take::<bool>(
                self,
                &owner_msg_id_key(&message.recipient, key),
                Column::OwnedMessageIds,
            )?;
        }

        Ok(result)
    }
}

impl DatabaseColumn for SpentMessages {
    fn column() -> Column {
        Column::SpentMessages
    }
}

impl Database {
    pub fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<Nonce>> + '_ {
        self.iter_all_filtered::<Vec<u8>, bool, _, _>(
            Column::OwnedMessageIds,
            Some(*owner),
            start_message_id.map(|msg_id| owner_msg_id_key(owner, &msg_id)),
            direction,
        )
        .map(|res| {
            res.map(|(key, _)| {
                Nonce::try_from(&key[Address::LEN..Address::LEN + Nonce::LEN])
                    .expect("key is always {Nonce::LEN} bytes")
            })
        })
    }

    pub fn all_messages(
        &self,
        start: Option<Nonce>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<Message>> + '_ {
        let start = start.map(|v| v.deref().to_vec());
        self.iter_all_by_start::<Vec<u8>, Message, _>(Column::Messages, start, direction)
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
                    sender: msg.sender,
                    recipient: msg.recipient,
                    nonce: msg.nonce,
                    amount: msg.amount,
                    data: msg.data,
                    da_height: msg.da_height,
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

// TODO: Reuse `fuel_vm::storage::double_key` macro.
/// Get a Key by chaining Owner + Nonce
fn owner_msg_id_key(owner: &Address, nonce: &Nonce) -> [u8; Address::LEN + Nonce::LEN] {
    let mut default = [0u8; Address::LEN + Nonce::LEN];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    default[Address::LEN..].copy_from_slice(nonce.as_ref());
    default
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::StorageAsMut;

    #[test]
    fn owned_message_ids() {
        let mut db = Database::default();
        let message = Message::default();

        // insert a message with the first id
        let first_id = 1.into();
        let _ = db
            .storage::<Messages>()
            .insert(&first_id, &message)
            .unwrap();

        // insert a message with the second id with the same Owner
        let second_id = 2.into();
        let _ = db
            .storage::<Messages>()
            .insert(&second_id, &message)
            .unwrap();

        // verify that 2 message IDs are associated with a single Owner/Recipient
        let owned_msg_ids = db.owned_message_ids(&message.recipient, None, None);
        assert_eq!(owned_msg_ids.count(), 2);

        // remove the first message with its given id
        let _ = db.storage::<Messages>().remove(&first_id).unwrap();

        // verify that only second ID is left
        let owned_msg_ids: Vec<_> = db
            .owned_message_ids(&message.recipient, None, None)
            .collect();
        assert_eq!(owned_msg_ids.first().unwrap().as_ref().unwrap(), &second_id);
        assert_eq!(owned_msg_ids.len(), 1);

        // remove the second message with its given id
        let _ = db.storage::<Messages>().remove(&second_id).unwrap();
        let owned_msg_ids = db.owned_message_ids(&message.recipient, None, None);
        assert_eq!(owned_msg_ids.count(), 0);
    }
}
