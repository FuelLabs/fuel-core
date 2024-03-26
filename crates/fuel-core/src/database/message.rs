use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    fuel_core_graphql_api::storage::messages::{
        OwnedMessageIds,
        OwnedMessageKey,
    },
};
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    tables::{
        Messages,
        SpentMessages,
    },
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::relayer::message::Message,
    fuel_types::{
        Address,
        Nonce,
    },
};
use itertools::Itertools;

impl Database<OffChain> {
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
}

impl Database {
    pub fn all_messages(
        &self,
        start: Option<Nonce>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<Message>> + '_ {
        self.iter_all_by_start::<Messages>(start.as_ref(), direction)
            .map(|res| res.map(|(_, message)| message))
    }

    pub fn iter_messages(
        &self,
    ) -> impl Iterator<Item = StorageResult<TableEntry<Messages>>> + '_ {
        self.iter_all_by_start::<Messages>(None, None)
            .filter_map(|msg| {
                // Return only unspent messages
                if let Ok(msg) = msg {
                    match self.message_is_spent(msg.1.id()) {
                        Ok(false) => Some(Ok(msg)),
                        Ok(true) => None,
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    Some(msg.map_err(StorageError::from))
                }
            })
            .map_ok(|(key, value)| TableEntry { key, value })
    }

    pub fn message_is_spent(&self, id: &Nonce) -> StorageResult<bool> {
        fuel_core_storage::StorageAsRef::storage::<SpentMessages>(&self).contains_key(id)
    }

    pub fn message_exists(&self, id: &Nonce) -> StorageResult<bool> {
        fuel_core_storage::StorageAsRef::storage::<Messages>(&self).contains_key(id)
    }
}
