use crate::client::schema::{schema, HexString256, PageInfo, U64};
use crate::client::{PageDirection, PaginatedResult, PaginationRequest};

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinByIdArgs {
    pub id: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "CoinByIdArgs"
)]
pub struct CoinByIdQuery {
    #[arguments(id = &args.id)]
    pub coin: Option<Coin>,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinsByOwnerConnectionArgs {
    /// Select coins based on the `owner` field
    pub owner: HexString256,
    /// Skip until coin id (forward pagination)
    pub after: Option<String>,
    /// Skip until coin id (backward pagination)
    pub before: Option<String>,
    /// Retrieve the first n coins in order (forward pagination)
    pub first: Option<i32>,
    /// Retrieve the last n coins in order (backward pagination).
    /// Can't be used at the same time as `first`.
    pub last: Option<i32>,
}

impl From<(HexString256, PaginationRequest<String>)> for CoinsByOwnerConnectionArgs {
    fn from(r: (HexString256, PaginationRequest<String>)) -> Self {
        match r.1.direction {
            PageDirection::Forward => CoinsByOwnerConnectionArgs {
                owner: r.0,
                after: r.1.cursor,
                before: None,
                first: Some(r.1.results as i32),
                last: None,
            },
            PageDirection::Backward => CoinsByOwnerConnectionArgs {
                owner: r.0,
                after: None,
                before: r.1.cursor,
                first: None,
                last: Some(r.1.results as i32),
            },
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "CoinsByOwnerConnectionArgs"
)]
pub struct CoinsQuery {
    #[arguments(owner = &args.owner, after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub coins_by_owner: CoinConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinConnection {
    pub edges: Option<Vec<Option<CoinEdge>>>,
    pub page_info: PageInfo,
}

impl From<CoinConnection> for PaginatedResult<Coin, String> {
    fn from(conn: CoinConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            results: conn
                .edges
                .unwrap_or_default()
                .into_iter()
                .filter_map(|e| e.map(|e| e.node))
                .collect(),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinEdge {
    pub cursor: String,
    pub node: Coin,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Coin {
    pub amount: U64,
    pub block_created: U64,
    pub color: HexString256,
    pub id: HexString256,
    pub maturity: U64,
    pub owner: HexString256,
    pub status: CoinStatus,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum CoinStatus {
    Unspent,
    Spent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinByIdQuery::build(CoinByIdArgs {
            id: HexString256::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn coins_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinsQuery::build(CoinsByOwnerConnectionArgs {
            owner: HexString256::default(),
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }
}
