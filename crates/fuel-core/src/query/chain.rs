use crate::graphql_api::ports::DatabasePort;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

pub trait ChainQueryData: Send + Sync {
    fn name(&self) -> StorageResult<String>;

    fn base_chain_height(&self) -> StorageResult<DaBlockHeight>;
}

impl<D: DatabasePort + ?Sized> ChainQueryData for D {
    fn name(&self) -> StorageResult<String> {
        self.chain_name()
    }

    fn base_chain_height(&self) -> StorageResult<DaBlockHeight> {
        self.base_chain_height()
    }
}
