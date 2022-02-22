use crate::client::schema::{schema, HexString256, HexStringUtxoId, PageInfo, U64};
use crate::client::{PageDirection, PaginatedResult, PaginationRequest};

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinByIdArgs {
    pub utxo_id: HexStringUtxoId,
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
    pub owner: HexString256,
    /// Filter coins based on the `color` field
    pub color: Option<HexString256>,
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

impl From<(HexString256, HexString256, PaginationRequest<String>)> for CoinsConnectionArgs {
    fn from(r: (HexString256, HexString256, PaginationRequest<String>)) -> Self {
        match r.2.direction {
            PageDirection::Forward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    color: Some(r.1),
                },
                after: r.2.cursor,
                before: None,
                first: Some(r.2.results as i32),
                last: None,
            },
            PageDirection::Backward => CoinsConnectionArgs {
                filter: CoinFilterInput {
                    owner: r.0,
                    color: Some(r.1),
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

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SpendQueryElementInput {
    /// color of the coins
    pub color: HexString256,
    /// address of the owner
    pub amount: U64,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct CoinsToSpendArgs {
    /// The Address of the utxo owner
    owner: HexString256,
    /// The total amount of each asset type to spend
    spend_query: Vec<SpendQueryElementInput>,
    /// The max number of utxos that can be used
    max_inputs: Option<i32>,
}

impl From<(HexString256, Vec<SpendQueryElementInput>, Option<i32>)> for CoinsToSpendArgs {
    fn from(r: (HexString256, Vec<SpendQueryElementInput>, Option<i32>)) -> Self {
        CoinsToSpendArgs {
            owner: r.0,
            spend_query: r.1,
            max_inputs: r.2,
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "CoinsToSpendArgs"
)]
pub struct CoinsToSpendQuery {
    #[arguments(owner = &args.owner, spend_query = &args.spend_query, max_inputs = &args.max_inputs)]
    pub coins_to_spend: Vec<Coin>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Coin {
    pub amount: U64,
    pub block_created: U64,
    pub color: HexString256,
    pub utxo_id: HexStringUtxoId,
    pub maturity: U64,
    pub owner: HexString256,
    pub status: CoinStatus,
}

#[derive(cynic::Enum, Clone, Copy, Debug, PartialEq)]
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
            utxo_id: HexStringUtxoId::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn coins_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = CoinsQuery::build(CoinsConnectionArgs {
            filter: CoinFilterInput {
                owner: HexString256::default(),
                color: HexString256::default().into(),
            },
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }
}
