use crate::graphql_api::ports::{
    OffChainDatabase,
    OnChainDatabase,
};
use fuel_core_storage::{
    not_found,
    tables::BlobData,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::fuel_tx::BlobId;

pub trait BlobQueryData: Send + Sync {
    fn blob_id(&self, id: BlobId) -> StorageResult<BlobId>;
    fn blob_bytecode(&self, id: BlobId) -> StorageResult<Vec<u8>>;
}

impl<D: OnChainDatabase + OffChainDatabase + ?Sized> BlobQueryData for D {
    fn blob_id(&self, id: BlobId) -> StorageResult<BlobId> {
        let blob_exists = self.storage::<BlobData>().contains_key(&id)?;

        if blob_exists {
            Ok(id)
        } else {
            Err(not_found!(BlobData))
        }
    }

    fn blob_bytecode(&self, id: BlobId) -> StorageResult<Vec<u8>> {
        let blob = self
            .storage::<BlobData>()
            .get(&id)?
            .ok_or(not_found!(BlobData))?
            .into_owned();

        Ok(blob.into())
    }
}
