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
    Provider: AtomicView<View = View>,
    View: OnChainDatabase + 'static,
{
    type View = OnChainView;

    fn view_at(&self, height: BlockHeight) -> StorageResult<Self::View> {
        let view = self.inner.view_at(height)?;
        Ok(Arc::new(view))
    }

    fn latest_view(&self) -> Self::View {
        Arc::new(self.inner.latest_view())
    }
}

impl<Provider, View> AtomicView for ArcWrapper<Provider, OffChainView>
where
    Provider: AtomicView<View = View>,
    View: OffChainDatabase + 'static,
{
    type View = OffChainView;

    fn view_at(&self, height: BlockHeight) -> StorageResult<Self::View> {
        let view = self.inner.view_at(height)?;
        Ok(Arc::new(view))
    }

    fn latest_view(&self) -> Self::View {
        Arc::new(self.inner.latest_view())
    }
}
