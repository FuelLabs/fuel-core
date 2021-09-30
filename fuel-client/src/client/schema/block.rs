use crate::client::schema::tx::Transaction;
use crate::client::schema::{schema, DateTime, HexString, HexString256};

#[derive(cynic::FragmentArguments, Debug)]
pub struct BlockByIdArgs {
    pub id: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "BlockByIdArgs"
)]
pub struct BlockByIdQuery {
    #[arguments(id = &args.id)]
    pub block: Option<Block>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Block {
    pub height: i32,
    pub id: HexString256,
    pub time: DateTime,
    pub producer: HexString256,
    pub transactions: Vec<Transaction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlockByIdQuery::build(BlockByIdArgs {
            id: HexString256("".to_string()),
        });
        insta::assert_snapshot!(operation.query)
    }
}
