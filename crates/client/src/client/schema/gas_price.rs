use crate::client::schema::{
    schema,
    U32,
    U64,
};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct LatestGasPrice {
    pub gas_price: U64,
    pub block_height: U32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct QueryLatestGasPrice {
    pub latest_gas_price: LatestGasPrice,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_gas_price_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = QueryLatestGasPrice::build(());
        insta::assert_snapshot!(operation.query)
    }
}
