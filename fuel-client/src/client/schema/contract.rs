use crate::client::schema::{schema, AssetId, ContractId, HexString, Salt, U64};

#[derive(cynic::FragmentArguments, Debug)]
pub struct ContractByIdArgs {
    pub id: ContractId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ContractByIdArgs"
)]
pub struct ContractByIdQuery {
    #[arguments(id = &args.id)]
    pub contract: Option<Contract>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalance {
    pub contract: ContractId,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct ContractBalanceQueryArgs {
    pub id: ContractId,
    pub asset: AssetId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ContractBalanceQueryArgs"
)]
pub struct ContractBalanceQuery {
    #[arguments(contract = &args.id, asset = &args.asset)]
    pub contract_balance: ContractBalance,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Contract {
    pub id: ContractId,
    pub bytecode: HexString,
    pub salt: Salt,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Contract")]
pub struct ContractIdFragment {
    pub id: ContractId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = ContractByIdQuery::build(ContractByIdArgs {
            id: ContractId::default(),
        });
        insta::assert_snapshot!(operation.query)
    }
}
