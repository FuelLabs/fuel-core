use crate::client::{
    schema::{
        schema,
        AssetId,
        ContractId,
        HexString,
        PageInfo,
        Salt,
        U64,
    },
    PageDirection,
    PaginationRequest,
};

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractByIdArgs {
    pub id: ContractId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractByIdArgs"
)]
pub struct ContractByIdQuery {
    #[arguments(id: $id)]
    pub contract: Option<Contract>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalance {
    pub contract: ContractId,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractBalanceQueryArgs {
    pub id: ContractId,
    pub asset: AssetId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractBalanceQueryArgs"
)]
pub struct ContractBalanceQuery {
    #[arguments(contract: $id, asset: $asset)]
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

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalanceFilterInput {
    /// Filter asset balances based on the `contract` field
    pub contract: ContractId,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractBalancesConnectionArgs {
    /// Filter balances based on a filter
    filter: ContractBalanceFilterInput,
    /// Skip until asset id (forward pagination)
    pub after: Option<String>,
    /// Skip until asset id (backward pagination)
    pub before: Option<String>,
    /// Retrieve the first n asset balances in order (forward pagination)
    pub first: Option<i32>,
    /// Retrieve the last n asset balances in order (backward pagination).
    /// Can't be used at the same time as `first`.
    pub last: Option<i32>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalanceEdge {
    pub cursor: String,
    pub node: ContractBalance,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalanceConnection {
    pub edges: Vec<ContractBalanceEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractBalancesConnectionArgs"
)]
pub struct ContractBalancesQuery {
    #[arguments(filter: $filter, after: $after, before: $before, first: $first, last: $last)]
    pub contract_balances: ContractBalanceConnection,
}

impl From<(ContractId, PaginationRequest<String>)> for ContractBalancesConnectionArgs {
    fn from(r: (ContractId, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => ContractBalancesConnectionArgs {
                filter: ContractBalanceFilterInput { contract: r.0 },
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results),
                last: None,
            },
            PageDirection::Backward => ContractBalancesConnectionArgs {
                filter: ContractBalanceFilterInput { contract: r.0 },
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results),
            },
        }
    }
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
