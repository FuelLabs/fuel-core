use crate::fuel_core_graphql_api::service::Database;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

pub struct ChainQueryContext<'a>(pub &'a Database);

pub trait ChainQueryData: Send + Sync {
    fn name(&self) -> StorageResult<String>;
    fn base_chain_height(&self) -> StorageResult<DaBlockHeight>;
}

impl ChainQueryData for ChainQueryContext<'_> {
    fn name(&self) -> StorageResult<String> {
        self.0.chain_name()
    }

    fn base_chain_height(&self) -> StorageResult<DaBlockHeight> {
        self.0.base_chain_height()
    }
}
