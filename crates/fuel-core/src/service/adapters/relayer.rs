use crate::database::{
    Database,
    database_description::relayer::Relayer,
};
use fuel_core_relayer::ports::Transactional;
use fuel_core_storage::{
    Result as StorageResult,
    StorageAsRef,
    transactional::{
        AtomicView,
        HistoricalView,
        IntoTransaction,
        StorageTransaction,
    },
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

    fn fcr_evidence(
        &self,
        height: &DaBlockHeight,
    ) -> StorageResult<Option<fuel_core_relayer::fcr::BlockEvidence>> {
        Ok(self
            .latest_view()?
            .storage_as_ref::<fuel_core_relayer::storage::EthereumEvidence>()
            .get(height)?
            .map(|value| value.into_owned()))
    }

    fn fcr_activated(&self) -> StorageResult<bool> {
        use crate::database::metadata::MetadataTable;
        Ok(self
            .storage_as_ref::<MetadataTable<Relayer>>()
            .get(&())?
            .is_some_and(|value| value.version() == 1))
    }

    fn activate_fcr(&mut self, anchor: DaBlockHeight) -> StorageResult<()> {
        self.activate_fcr_metadata(anchor)
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
