use crate::client::schema::{
    schema,
    U32,
    U64,
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct LatestGasPrice {
    pub gas_price: U64,
    pub block_height: U32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct QueryLatestGasPrice {
    pub latest_gas_price: LatestGasPrice,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct EstimateGasPrice {
    pub gas_price: U64,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct BlockHorizonArgs {
    pub block_horizon: Option<U32>,
}

impl From<u32> for BlockHorizonArgs {
    fn from(block_horizon: u32) -> Self {
        Self {
            block_horizon: Some(block_horizon.into()),
        }
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "BlockHorizonArgs"
)]
pub struct QueryEstimateGasPrice {
    #[arguments(blockHorizon: $block_horizon)]
    pub estimate_gas_price: EstimateGasPrice,
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

    #[test]
    fn estimate_gas_price_query_gql_output() {
        use cynic::QueryBuilder;
        let arbitrary_horizon = 10;
        let operation = QueryEstimateGasPrice::build(BlockHorizonArgs {
            block_horizon: Some(arbitrary_horizon.into()),
        });
        insta::assert_snapshot!(operation.query)
    }
}
