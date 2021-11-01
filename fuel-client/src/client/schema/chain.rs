use crate::client::schema::{block::Block, schema, U64};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct ChainQuery {
    pub chain: ChainInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChainInfo {
    pub base_chain_height: U64,
    pub name: String,
    pub peer_count: i32,
    pub latest_block: Block,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_gql_query_output() {
        use cynic::QueryBuilder;
        let operation = ChainQuery::build(());
        insta::assert_snapshot!(operation.query)
    }
}
