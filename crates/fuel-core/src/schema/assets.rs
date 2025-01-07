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
            ContractId,
            SubId,
            U128,
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
    ) -> async_graphql::Result<AssetInfoDetails> {
        let query = ctx.read_view()?;
        query
            .get_asset_details(&id.into())
            .map(|details| details.into())
            .map_err(async_graphql::Error::from)
    }
}

#[derive(Clone, Debug)]
pub struct AssetInfoDetails {
    pub contract_id: ContractId,
    pub sub_id: SubId,
    pub total_supply: U128,
}

impl From<AssetDetails> for AssetInfoDetails {
    fn from(details: AssetDetails) -> Self {
        AssetInfoDetails {
            contract_id: details.contract_id.into(),
            sub_id: details.sub_id.into(),
            total_supply: details.total_supply.into(),
        }
    }
}

#[Object]
impl AssetInfoDetails {
    async fn contract_id(&self) -> &ContractId {
        &self.contract_id
    }

    async fn sub_id(&self) -> &SubId {
        &self.sub_id
    }

    async fn total_supply(&self) -> &U128 {
        &self.total_supply
    }
}
