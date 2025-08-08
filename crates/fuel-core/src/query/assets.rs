use crate::{
    database::database_description::IndexationKind,
    fuel_core_graphql_api::database::ReadView,
    graphql_api::storage::assets::AssetDetails,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_tx::AssetId;

impl ReadView {
    pub fn get_asset_details(&self, id: &AssetId) -> StorageResult<Option<AssetDetails>> {
        if self
            .indexation_flags
            .contains(&IndexationKind::AssetMetadata)
        {
            let maybe_asset_details = self.off_chain.asset_info(id)?;
            Ok(maybe_asset_details)
        } else {
            Err(anyhow::anyhow!("Asset metadata index is not available").into())
        }
    }
}
