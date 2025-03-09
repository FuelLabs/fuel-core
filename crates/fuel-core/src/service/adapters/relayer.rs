use crate::database::{
    database_description::relayer::Relayer,
    Database,
};
use fuel_core_relayer::ports::Transactional;
use fuel_core_storage::{
    transactional::{
        AtomicView,
        HistoricalView,
        IntoTransaction,
        StorageTransaction,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

impl Transactional for Database<Relayer> {
    type Transaction<'a>
        = StorageTransaction<&'a mut Self>
    where
        Self: 'a;

    fn transaction(&mut self) -> Self::Transaction<'_> {
        self.into_transaction()
    }

    fn latest_da_height(&self) -> Option<DaBlockHeight> {
        HistoricalView::latest_height(self)
    }
}

impl Database<Relayer> {
    pub fn get_events(&self, da_height: &DaBlockHeight) -> StorageResult<Vec<Event>> {
        let events = self
            .latest_view()?
            .storage_as_ref::<fuel_core_relayer::storage::EventsHistory>()
            .get(da_height)?
            .unwrap_or_default()
            .into_owned();

        Ok(events)
    }
}
