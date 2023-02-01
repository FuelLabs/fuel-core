use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_types::MessageId,
};

pub trait RelayerPort {
    fn get_message(
        &self,
        id: &MessageId,
        da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>>;
}

#[cfg(test)]
impl RelayerPort for crate::database::Database {
    fn get_message(
        &self,
        id: &MessageId,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsRef,
        };
        use std::borrow::Cow;
        Ok(self.storage::<Messages>().get(id)?.map(Cow::into_owned))
    }
}
