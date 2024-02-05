use crate::fuel_core_graphql_api::ports::OnChainDatabase;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

pub trait ChainQueryData: Send + Sync {
    fn da_height(&self) -> StorageResult<DaBlockHeight>;
}

impl<D: OnChainDatabase + ?Sized> ChainQueryData for D {
    fn da_height(&self) -> StorageResult<DaBlockHeight> {
        self.da_height()
    }
}
