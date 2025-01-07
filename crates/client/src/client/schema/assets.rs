use crate::client::schema::{
    schema,
    AssetId,
    ContractId,
    SubId,
    U128,
};

#[derive(cynic::QueryVariables, Debug)]
pub struct AssetInfoArg {
    pub id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "AssetInfoArg"
)]
pub struct AssetInfoQuery {
    #[arguments(id: $id)]
    pub asset_details: AssetInfoDetails,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct AssetInfoDetails {
    pub sub_id: SubId,
    pub contract_id: ContractId,
    pub total_supply: U128,
}
