use crate::client::schema::{
    schema, tx::OpaqueTransaction, ConnectionArgs, DateTime, HexString256, PageInfo, U64,
};

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
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ConnectionArgs"
)]
pub struct BlocksQuery {
    #[arguments(after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub blocks: BlockConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct BlockConnection {
    pub edges: Option<Vec<Option<BlockEdge>>>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct BlockEdge {
    pub cursor: String,
    pub node: Block,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Block {
    pub height: U64,
    pub id: HexString256,
    pub time: DateTime,
    pub producer: HexString256,
    pub transactions: Vec<OpaqueTransaction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlockByIdQuery::build(BlockByIdArgs {
            id: HexString256::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn blocks_connection_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlocksQuery::build(ConnectionArgs {
            after: None,
            before: None,
            first: None,
            last: None,
        });
        insta::assert_snapshot!(operation.query)
    }
}
