use crate::{
    fuel_core_graphql_api::database::ReadView,
    graphql_api::storage::assets::AssetDetails,
};
use fuel_core_storage::{
    not_found,
    Result as StorageResult,
};
use fuel_core_types::fuel_tx::AssetId;

impl ReadView {
    pub fn get_asset_details(&self, id: &AssetId) -> StorageResult<AssetDetails> {
        if self.asset_metadata_indexation_enabled {
            Ok(self
                .off_chain
                .asset_info(id)?
                .ok_or(not_found!(AssetDetails))?)
        } else {
            Err(anyhow::anyhow!("Asset metadata index is not available").into())
        }
    }
}
