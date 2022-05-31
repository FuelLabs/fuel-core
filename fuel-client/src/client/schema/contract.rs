use crate::client::schema::{schema, AssetId, ContractId, HexString, PageInfo, Salt, U64};
use crate::client::{PageDirection, PaginatedResult, PaginationRequest};

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

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractBalanceFilterInput {
    /// Filter asset balances based on the `contract` field
    pub contract: ContractId,
}

#[derive(cynic::FragmentArguments, Debug)]
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
    argument_struct = "ContractBalancesConnectionArgs"
)]
pub struct ContractBalancesQuery {
    #[arguments(filter = &args.filter, after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub contract_balances: ContractBalanceConnection,
}

impl From<ContractBalanceConnection> for PaginatedResult<ContractBalance, String> {
    fn from(conn: ContractBalanceConnection) -> Self {
        PaginatedResult {
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            cursor: conn.page_info.end_cursor,
            results: conn.edges.into_iter().map(|e| e.node).collect(),
        }
    }
}

impl From<(ContractId, PaginationRequest<String>)> for ContractBalancesConnectionArgs {
    fn from(r: (ContractId, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => ContractBalancesConnectionArgs {
                filter: ContractBalanceFilterInput { contract: r.0 },
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results as i32),
                last: None,
            },
            PageDirection::Backward => ContractBalancesConnectionArgs {
                filter: ContractBalanceFilterInput { contract: r.0 },
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results as i32),
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
