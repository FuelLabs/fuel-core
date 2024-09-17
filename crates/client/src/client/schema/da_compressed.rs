use crate::client::schema::{
    schema,
    U32,
};

use super::HexString;

#[derive(cynic::QueryVariables, Debug)]
pub struct DaCompressedBlockByHeightArgs {
    pub height: U32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "DaCompressedBlockByHeightArgs"
)]
pub struct DaCompressedBlockByHeightQuery {
    #[arguments(height: $height)]
    pub da_compressed_block: Option<DaCompressedBlock>,
}

/// Block with transaction ids
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct DaCompressedBlock {
    pub bytes: HexString,
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn block_by_id_query_gql_output() {
//         use cynic::QueryBuilder;
//         let operation = BlockByIdQuery::build(BlockByIdArgs {
//             id: Some(BlockId::default()),
//         });
//         insta::assert_snapshot!(operation.query)
//     }

//     #[test]
//     fn block_by_height_query_gql_output() {
//         use cynic::QueryBuilder;
//         let operation = BlockByHeightQuery::build(BlockByHeightArgs {
//             height: Some(U32(0)),
//         });
//         insta::assert_snapshot!(operation.query)
//     }

//     #[test]
//     fn block_mutation_query_gql_output() {
//         use cynic::MutationBuilder;
//         let operation = BlockMutation::build(ProduceBlockArgs {
//             blocks_to_produce: U32(0),
//             start_timestamp: None,
//         });
//         insta::assert_snapshot!(operation.query)
//     }

//     #[test]
//     fn blocks_connection_query_gql_output() {
//         use cynic::QueryBuilder;
//         let operation = BlocksQuery::build(ConnectionArgs {
//             after: None,
//             before: None,
//             first: None,
//             last: None,
//         });
//         insta::assert_snapshot!(operation.query)
//     }
// }
