use crate::database::{
    metadata,
    Column,
    Database,
};
use fuel_core_relayer::ports::RelayerDb;
use fuel_core_storage::{
    not_found,
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
};

impl RelayerDb for Database {
    fn insert_message(
        &mut self,
        message: &CheckedMessage,
    ) -> StorageResult<Option<Message>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsMut,
        };

        self.storage::<Messages>()
            .insert(message.id(), message.message())
    }

    fn set_finalized_da_height(&mut self, block: DaBlockHeight) -> StorageResult<()> {
        let _: Option<BlockHeight> =
            self.insert(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata, &block)?;
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        self.get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata)?
            .ok_or(not_found!("FinalizedDaHeight"))
    }
}
