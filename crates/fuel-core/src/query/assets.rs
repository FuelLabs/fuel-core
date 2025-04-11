use crate::{
    database::database_description::IndexationKind,
    fuel_core_graphql_api::database::ReadView,
    graphql_api::storage::assets::AssetDetails,
};
use fuel_core_storage::{
    Result as StorageResult,
    not_found,
};
use fuel_core_types::fuel_tx::AssetId;

impl ReadView {
    pub fn get_asset_details(&self, id: &AssetId) -> StorageResult<AssetDetails> {
        if self
            .indexation_flags
            .contains(&IndexationKind::AssetMetadata)
        {
            Ok(self
                .off_chain
                .asset_info(id)?
                .ok_or(not_found!(AssetDetails))?)
        } else {
            Err(anyhow::anyhow!("Asset metadata index is not available").into())
        }
    }
}
