use crate::{
    chain_config::MessageConfig,
    database::{
        Column,
        Database,
        KvStoreError,
    },
    multikey,
    state::{
        Error,
        IterDirection,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_types::{
            Address,
            Bytes32,
            MessageId,
        },
    },
    db::Messages,
    model::Message,
};
use std::{
    borrow::Cow,
    ops::Deref,
};

impl StorageInspect<Messages> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &MessageId) -> Result<Option<Cow<Message>>, KvStoreError> {
        self._get(key.as_ref(), Column::Messages)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &MessageId) -> Result<bool, KvStoreError> {
        self._contains_key(key.as_ref(), Column::Messages)
            .map_err(Into::into)
    }
}

impl StorageMutate<Messages> for Database {
    fn insert(
        &mut self,
        key: &MessageId,
        value: &Message,
    ) -> Result<Option<Message>, KvStoreError> {
        // insert primary record
        let result = self._insert(key.as_ref(), Column::Messages, value)?;

        // insert secondary record by owner
        let _: Option<bool> = self._insert(
            multikey!(&value.recipient, Address, key, MessageId).as_ref(),
            Column::OwnedMessageIds,
            &true,
        )?;

        Ok(result)
    }

    fn remove(&mut self, key: &MessageId) -> Result<Option<Message>, KvStoreError> {
        let result: Option<Message> = self._remove(key.as_ref(), Column::Messages)?;

        if let Some(message) = &result {
            Database::_remove::<bool>(
                self,
                multikey!(&message.recipient, Address, key, MessageId).as_ref(),
                Column::OwnedMessageIds,
            )?;
        }

        Ok(result)
    }
}

impl Database {
    pub fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<MessageId, Error>> + '_ {
        self.iter_all::<Vec<u8>, bool>(
            Column::OwnedMessageIds,
            Some(owner.to_vec()),
            start_message_id.map(|msg_id| crate::multikey!(owner, Address, &msg_id, MessageId).to_vec()),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| {
            res.map(|(key, _)| {
                MessageId::new(unsafe { *Bytes32::from_slice_unchecked(&key[32..64]) })
            })
        })
    }

    pub fn all_messages(
        &self,
        start: Option<MessageId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Message, Error>> + '_ {
        self.iter_all::<Vec<u8>, Message>(
            Column::Messages,
            None,
            start.map(|v| v.deref().to_vec()),
            direction,
        )
        .map(|res| res.map(|(_, message)| message))
    }

    pub fn get_message_config(&self) -> Result<Option<Vec<MessageConfig>>, Error> {
        let configs = self
            .all_messages(None, None)
            .map(|msg| -> Result<MessageConfig, Error> {
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
            .collect::<Result<Vec<MessageConfig>, Error>>()?;

        Ok(Some(configs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::fuel_storage::StorageAsMut;

    #[test]
    fn owned_message_ids() {
        let mut db = Database::default();
        let message = Message::default();

        // insert a message with the first id
        let first_id = MessageId::new([1; 32]);
        let _ = db
            .storage::<Messages>()
            .insert(&first_id, &message)
            .unwrap();

        // insert a message with the second id with the same Owner
        let second_id = MessageId::new([2; 32]);
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
