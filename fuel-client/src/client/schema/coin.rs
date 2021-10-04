use crate::client::schema::{schema, HexString256, PageInfo};

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
    pub owner: HexString256,
    pub after: Option<String>,
    pub before: Option<String>,
    pub first: Option<i32>,
    pub last: Option<i32>,
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

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinEdge {
    pub cursor: String,
    pub node: Coin,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Coin {
    pub amount: i32,
    pub block_created: i32,
    pub color: HexString256,
    pub id: HexString256,
    pub maturity: i32,
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
