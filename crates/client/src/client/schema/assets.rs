use crate::client::schema::{schema, AssetId, HexString, U64};

#[derive(cynic::QueryVariables, Debug)]
pub struct AssetInfoArg {
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "AssetInfoArg"
)]
pub struct AssetInfoQuery {
    #[arguments(id: $id)]
    pub asset_details: Option<AssetInfoDetails>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct AssetInfoDetails {
    pub sub_id: HexString,
    pub contract_id: HexString,
    pub total_supply: U64,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct AssetIdFragment {
    pub id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "AssetInfoArg"
)]
pub struct AssetExistsQuery {
    #[arguments(id: $id)]
    pub asset_id: Option<bool>,
}
