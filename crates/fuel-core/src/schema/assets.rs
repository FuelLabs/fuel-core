use async_graphql::{
    Context,
    Object,
};

use crate::{
    fuel_core_graphql_api::query_costs,
    graphql_api::storage::assets::AssetDetails,
    schema::{
        scalars::{
            AssetId,
            HexString,
            U64,
        },
        ReadViewProvider,
    },
};

#[derive(Default)]
pub struct AssetInfoQuery;

#[Object]
impl AssetInfoQuery {
    #[graphql(complexity = "query_costs().storage_read")]
    async fn asset_details(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the Asset")] id: AssetId,
    ) -> async_graphql::Result<Option<AssetInfoDetails>> {
        let query = ctx.read_view()?;
        query
            .get_asset_details(id.into())
            .map(|details| Some(details.into()))
            .map_err(async_graphql::Error::from)
    }
}

#[derive(Clone, Debug)]
pub struct AssetInfoDetails {
    pub contract_id: HexString,
    pub sub_id: HexString,
    pub total_supply: U64,
}

impl From<AssetDetails> for AssetInfoDetails {
    fn from(details: AssetDetails) -> Self {
        AssetInfoDetails {
            contract_id: details.0.as_ref().to_vec().into(),
            sub_id: details.1.as_ref().to_vec().into(),
            total_supply: details.2.into(),
        }
    }
}

#[Object]
impl AssetInfoDetails {
    async fn contract_id(&self) -> &HexString {
        &self.contract_id
    }

    async fn sub_id(&self) -> &HexString {
        &self.sub_id
    }

    async fn total_supply(&self) -> &U64 {
        &self.total_supply
    }
}
