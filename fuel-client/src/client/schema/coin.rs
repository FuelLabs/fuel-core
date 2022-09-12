use crate::client::{
    schema::{
        schema,
        Address,
        AssetId,
        PageInfo,
        UtxoId,
        U64,
    },
    PageDirection,
    PaginatedResult,
    PaginationRequest,
};

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinByIdArgs {
    pub utxo_id: UtxoId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "CoinByIdArgs"
)]
pub struct CoinByIdQuery {
    #[arguments(utxo_id = &args.utxo_id)]
    pub coin: Option<Coin>,
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinFilterInput {
    /// Filter coins based on the `owner` field
    pub owner: Address,
    /// Filter coins based on the `asset_id` field
    pub asset_id: Option<AssetId>,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinsConnectionArgs {
    /// Filter coins based on a filter
    filter: CoinFilterInput,
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

impl From<(Address, AssetId, PaginationRequest<String>)> for CoinsConnectionArgs {
    fn from(r: (Address, AssetId, PaginationRequest<String>)) -> Self {
        match r.2.direction {
            PageDirection::Forward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    asset_id: Some(r.1),
                },
                after: r.2.cursor,
                before: None,
                first: Some(r.2.results as i32),
                last: None,
            },
            PageDirection::Backward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    asset_id: Some(r.1),
                },
                after: None,
                before: r.2.cursor,
                first: None,
                last: Some(r.2.results as i32),
            },
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "CoinsConnectionArgs"
)]
pub struct CoinsQuery {
    #[arguments(filter = &args.filter, after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub coins: CoinConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinConnection {
    pub edges: Vec<CoinEdge>,
    pub page_info: PageInfo,
}

impl From<CoinConnection> for PaginatedResult<Coin, String> {
    fn from(conn: CoinConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node).collect(),
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
    pub asset_id: AssetId,
    pub utxo_id: UtxoId,
    pub maturity: U64,
    pub owner: Address,
    pub status: CoinStatus,
}

#[derive(cynic::Enum, Clone, Copy, Debug, Eq, PartialEq)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum CoinStatus {
    Unspent,
    Spent,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Coin")]
pub struct CoinIdFragment {
    pub utxo_id: UtxoId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinByIdQuery::build(CoinByIdArgs {
            utxo_id: UtxoId::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn coins_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinsQuery::build(CoinsConnectionArgs {
            filter: CoinFilterInput {
                owner: Address::default(),
                asset_id: Some(AssetId::default()),
            },
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }
}
