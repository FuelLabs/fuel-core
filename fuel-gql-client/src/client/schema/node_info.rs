use crate::client::schema::{schema, U64};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct NodeInfo {
    pub utxo_validation: bool,
    pub predicates: bool,
    pub vm_backtrace: bool,
    pub min_gas_price: U64,
    pub min_byte_price: U64,
    pub max_tx: U64,
    pub max_depth: U64,
    pub node_version: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct QueryNodeInfo {
    pub node_info: NodeInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_info_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = QueryNodeInfo::build(());
        insta::assert_snapshot!(operation.query)
    }
}
