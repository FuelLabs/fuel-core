use crate::{
    fuel_core_graphql_api::{
        database::{
            OffChainView,
            OffChainViewAt,
            OnChainView,
        },
        ports::{
            OffChainDatabase,
            OffChainDatabaseAt,
            OnChainDatabase,
            OnChainDatabaseAt,
        },
    },
    graphql_api::database::OnChainViewAt,
};
use fuel_core_storage::{
    Result as StorageResult,
    transactional::{
        AtomicView,
        HistoricalView,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;

/// The GraphQL can't work with the generics in [`async_graphql::Context::data_unchecked`] and requires a known type.
/// It is an `Arc` wrapper around the generic for on-chain and off-chain databases.
pub struct ArcWrapper<Provider, ArcView> {
    inner: Provider,
    _marker: core::marker::PhantomData<ArcView>,
}

impl<Provider, ArcView> ArcWrapper<Provider, ArcView> {
    pub fn new(inner: Provider) -> Self {
        Self {
            inner,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<Provider, View> AtomicView for ArcWrapper<Provider, OnChainView>
where
    Provider: AtomicView<LatestView = View>,
    View: OnChainDatabase + 'static,
{
    type LatestView = OnChainView;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(Arc::new(self.inner.latest_view()?))
    }
}

impl<Provider> HistoricalView for ArcWrapper<Provider, OnChainView>
where
    Provider: HistoricalView<Height = BlockHeight>,
    Provider::LatestView: OnChainDatabase + 'static,
    Provider::ViewAtHeight: OnChainDatabaseAt + 'static,
{
    type ViewAtHeight = OnChainViewAt;

    type Height = BlockHeight;

    fn latest_height(&self) -> Option<Self::Height> {
        self.inner.latest_height()
    }

    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::ViewAtHeight> {
        Ok(Arc::new(self.inner.view_at(height)?))
    }
}

impl<Provider, View> AtomicView for ArcWrapper<Provider, OffChainView>
where
    Provider: AtomicView<LatestView = View>,
    View: OffChainDatabase + 'static,
{
    type LatestView = OffChainView;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(Arc::new(self.inner.latest_view()?))
    }
}

impl<Provider> HistoricalView for ArcWrapper<Provider, OffChainView>
where
    Provider: HistoricalView<Height = BlockHeight>,
    Provider::LatestView: OffChainDatabase + 'static,
    Provider::ViewAtHeight: OffChainDatabaseAt + 'static,
{
    type ViewAtHeight = OffChainViewAt;

    type Height = BlockHeight;

    fn latest_height(&self) -> Option<Self::Height> {
        self.inner.latest_height()
    }

    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::ViewAtHeight> {
        Ok(Arc::new(self.inner.view_at(height)?))
    }
}
