use crate::database::{
    database_description::relayer::Relayer,
    Database,
};
use fuel_core_relayer::ports::Transactional;
use fuel_core_storage::transactional::{
    HistoricalView,
    IntoTransaction,
    StorageTransaction,
};
use fuel_core_types::blockchain::primitives::DaBlockHeight;

impl Transactional for Database<Relayer> {
    type Transaction<'a> = StorageTransaction<&'a mut Self> where Self: 'a;

    fn transaction(&mut self) -> Self::Transaction<'_> {
        self.into_transaction()
    }

    fn latest_da_height(&self) -> Option<DaBlockHeight> {
        HistoricalView::latest_height(self)
    }
}
