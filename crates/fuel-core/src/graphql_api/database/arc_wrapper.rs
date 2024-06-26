use crate::fuel_core_graphql_api::{
    database::{
        OffChainView,
        OnChainView,
    },
    ports::{
        OffChainDatabase,
        OnChainDatabase,
    },
};
use fuel_core_storage::{
    transactional::AtomicView,
    Result as StorageResult,
};
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

impl<Provider, View, Height> AtomicView for ArcWrapper<Provider, OnChainView>
where
    Provider: AtomicView<LatestView = View, Height = Height>,
    View: OnChainDatabase + 'static,
{
    type LatestView = OnChainView;
    type Height = Height;

    fn latest_height(&self) -> Option<Self::Height> {
        self.inner.latest_height()
    }

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(Arc::new(self.inner.latest_view()?))
    }
}

impl<Provider, View, Height> AtomicView for ArcWrapper<Provider, OffChainView>
where
    Provider: AtomicView<LatestView = View, Height = Height>,
    View: OffChainDatabase + 'static,
{
    type LatestView = OffChainView;
    type Height = Height;

    fn latest_height(&self) -> Option<Self::Height> {
        self.inner.latest_height()
    }

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(Arc::new(self.inner.latest_view()?))
    }
}
