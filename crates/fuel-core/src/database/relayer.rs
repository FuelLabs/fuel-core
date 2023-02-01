use crate::database::{
    metadata,
    Column,
    Database,
};
use fuel_core_relayer::ports::RelayerDb;
use fuel_core_storage::{
    not_found,
    transactional::Transaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::primitives::{
        BlockHeight,
        DaBlockHeight,
    },
    entities::message::{
        CheckedMessage,
        Message,
    },
    fuel_types::MessageId,
};

#[cfg(test)]
mod tests;

impl RelayerDb for Database {
    fn insert_messages(
        &mut self,
        messages: &[(DaBlockHeight, CheckedMessage)],
    ) -> StorageResult<()> {
        let mut db = self.transaction();
        let mut key = [0u8; core::mem::size_of::<u64>() + MessageId::LEN];

        let mut max_height = None;
        for (height, message) in messages {
            key[..8].copy_from_slice(&height.0.to_be_bytes());
            key[8..].copy_from_slice(message.id().as_ref());
            let _: Option<Message> =
                db.insert(key, Column::RelayerMessages, message.message())?;
            let max = max_height.get_or_insert(0u64);
            *max = (*max).max(height.0);
        }
        if let Some(height) = max_height {
            let _: Option<BlockHeight> = db.insert(
                metadata::FINALIZED_DA_HEIGHT_KEY,
                Column::Metadata,
                BlockHeight::from(height),
            )?;
        }
        db.commit()?;
        Ok(())
    }

    fn set_finalized_da_height(&mut self, block: DaBlockHeight) -> StorageResult<()> {
        let _: Option<BlockHeight> =
            self.insert(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata, block)?;
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        self.get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata)?
            .ok_or(not_found!("FinalizedDaHeight"))
    }
}
