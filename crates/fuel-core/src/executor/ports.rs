use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::CompressedMessage,
    fuel_types::MessageId,
};

pub trait RelayerPort {
    /// Get a message from the relayer if it has been
    /// synced and is <= the given da height.
    fn get_message(
        &self,
        id: &MessageId,
        da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<CompressedMessage>>;
}

#[cfg(test)]
/// For some tests we don't care about the actual
/// implementation of the RelayerPort
/// and using a passthrough is fine.
impl RelayerPort for crate::database::Database {
    fn get_message(
        &self,
        id: &MessageId,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<CompressedMessage>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsRef,
        };
        use std::borrow::Cow;
        Ok(self.storage::<Messages>().get(id)?.map(Cow::into_owned))
    }
}
