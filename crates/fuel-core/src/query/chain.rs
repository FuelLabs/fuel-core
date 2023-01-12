use crate::fuel_core_graphql_api::service::Database;
use fuel_core_storage::Result as StorageResult;

pub struct ChainQueryContext<'a>(pub &'a Database);

impl ChainQueryContext<'_> {
    pub fn name(&self) -> StorageResult<String> {
        self.0.chain_name()
    }
}
