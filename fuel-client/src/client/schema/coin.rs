use crate::client::schema::{schema, HexString256};

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
            id: HexString256("".to_string()),
        });
        insta::assert_snapshot!(operation.query)
    }
}
