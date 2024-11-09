use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    not_found,
    tables::BlobData,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::fuel_tx::BlobId;

impl ReadView {
    pub fn blob_exists(&self, id: BlobId) -> StorageResult<bool> {
        self.storage::<BlobData>().contains_key(&id)
    }

    pub fn blob_bytecode(&self, id: BlobId) -> StorageResult<Vec<u8>> {
        let blob = self
            .storage::<BlobData>()
            .get(&id)?
            .ok_or(not_found!(BlobData))?
            .into_owned();

        Ok(blob.into())
    }
}
