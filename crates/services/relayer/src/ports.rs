//! Ports used by the relayer to access the outside world

use async_trait::async_trait;
use fuel_core_storage::{
    not_found,
    tables::Messages,
    transactional::Transactional,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::CheckedMessage,
};

#[cfg(test)]
mod tests;

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: Send + Sync {
    /// Add bridge messages to database. Messages are not revertible.
    /// Must set the maximum da height for these messages in
    /// the same write transaction.
    fn insert_messages(&mut self, messages: &[CheckedMessage]) -> StorageResult<()>;

    /// set finalized da height that represent last block from da layer that got finalized.
    fn set_finalized_da_height(&mut self, block: DaBlockHeight) -> StorageResult<()>;

    /// Assume it is always set as initialization of database.
    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
}

impl<T> RelayerDb for T
where
    T: StorageMutate<Messages, Error = fuel_core_storage::Error>
        + StorageMutate<RelayerMetadata, Error = fuel_core_storage::Error>
        + Transactional
        + Send
        + Sync,
    <T as Transactional>::Storage: StorageMutate<Messages, Error = fuel_core_storage::Error>
        + StorageMutate<RelayerMetadata, Error = fuel_core_storage::Error>,
{
    fn insert_messages(&mut self, messages: &[CheckedMessage]) -> StorageResult<()> {
        let mut db = self.transaction();

        let mut max_height = None;
        for message in messages {
            StorageAsMut::storage::<Messages>(&mut db.as_mut())
                .insert(message.id(), message.message())?;
            let max = max_height.get_or_insert(0u64);
            *max = (*max).max(message.message().da_height.0);
        }
        if let Some(height) = max_height {
            StorageAsMut::storage::<RelayerMetadata>(&mut db.as_mut())
                .insert(DA_HEIGHT_KEY, &DaBlockHeight::from(height))?;
        }
        db.commit()?;
        Ok(())
    }

    fn set_finalized_da_height(&mut self, block: DaBlockHeight) -> StorageResult<()> {
        self.storage::<RelayerMetadata>()
            .insert(DA_HEIGHT_KEY, &block)?;
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        Ok(*StorageAsRef::storage::<RelayerMetadata>(&self)
            .get(DA_HEIGHT_KEY)?
            .ok_or(not_found!("DaBlockHeight missing for relayer"))?)
    }
}

/// Todo
pub struct RelayerMetadata;
impl Mappable for RelayerMetadata {
    type Key = [u8];
    type OwnedKey = Vec<u8>;
    type Value = Self::OwnedValue;
    type OwnedValue = DaBlockHeight;
}

/// Key is set by trait implementor.
/// TODO: Add trait to fuel_storage that allows
/// the implementor to set the key.
const DA_HEIGHT_KEY: &[u8] = b"";
