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
        // insert primary record
        let result = Database::insert(self, key.as_ref(), columns::DA_MESSAGES, value.clone())?;

        // insert secondary record by owner
        Database::insert(
            self,
            owner_msg_id_key(&value.owner, key),
            columns::OWNED_MESSAGE_IDS,
            true,
        )?;

        Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_message_ids() {
        let mut db = Database::default();
        let da_msg = DaMessage::default();

        // insert a message with the first id
        let first_id = MessageId::new([1; 32]);
        let _ = Storage::<MessageId, DaMessage>::insert(&mut db, &first_id, &da_msg).unwrap();

        // insert a message with the second id with the same Owner
        let second_id = MessageId::new([2; 32]);
        let _ = Storage::<MessageId, DaMessage>::insert(&mut db, &second_id, &da_msg).unwrap();

        // verify that 2 message IDs are associated with a single Owner
        let owned_msg_ids = db.owned_message_ids(da_msg.owner, None, None);
        assert_eq!(owned_msg_ids.count(), 2);

        // remove the first message with its given id
        let _ = Storage::<MessageId, DaMessage>::remove(&mut db, &first_id).unwrap();

        // verify that only second ID is left
        let owned_msg_ids: Vec<_> = db.owned_message_ids(da_msg.owner, None, None).collect();
        assert_eq!(owned_msg_ids.first().unwrap().as_ref().unwrap(), &second_id);
        assert_eq!(owned_msg_ids.len(), 1);

        // remove the second message with its given id
        let _ = Storage::<MessageId, DaMessage>::remove(&mut db, &second_id).unwrap();
        let owned_msg_ids = db.owned_message_ids(da_msg.owner, None, None);
        assert_eq!(owned_msg_ids.count(), 0);
    }
}
