use crate::client::{
    schema::{
        schema,
        BlockId,
        ConnectionArgs,
        PageInfo,
        Signature,
        Tai64Timestamp,
        U64,
    },
    PaginatedResult,
};

use super::{
    tx::TransactionIdFragment,
    Bytes32,
};

#[derive(cynic::FragmentArguments, Debug)]
pub struct BlockByIdArgs {
    pub id: BlockId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "BlockByIdArgs"
)]
pub struct BlockByIdQuery {
    #[arguments(id = & args.id)]
    pub block: Option<Block>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ConnectionArgs"
)]
pub struct BlocksQuery {
    #[arguments(after = & args.after, before = & args.before, first = & args.first, last = & args.last)]
    pub blocks: BlockConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct BlockConnection {
    pub edges: Vec<BlockEdge>,
    pub page_info: PageInfo,
}

impl From<BlockConnection> for PaginatedResult<Block, String> {
    fn from(conn: BlockConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node).collect(),
        }
    }
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
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<TransactionIdFragment>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Block")]
pub struct BlockIdFragment {
    pub id: BlockId,
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TimeParameters {
    pub start_time: U64,
    pub block_time_interval: U64,
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct ProduceBlockArgs {
    pub blocks_to_produce: U64,
    pub time: Option<TimeParameters>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    argument_struct = "ProduceBlockArgs",
    graphql_type = "Mutation"
)]
pub struct BlockMutation {
    #[arguments(blocks_to_produce = &args.blocks_to_produce, time = &args.time)]
    pub produce_blocks: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Header {
    pub id: BlockId,
    pub da_height: U64,
    pub transactions_count: U64,
    pub output_messages_count: U64,
    pub transactions_root: Bytes32,
    pub output_messages_root: Bytes32,
    pub height: U64,
    pub prev_root: Bytes32,
    pub time: Tai64Timestamp,
    pub application_hash: Bytes32,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Consensus {
    PoAConsensus(PoAConsensus),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PoAConsensus {
    pub signature: Signature,
}

impl Block {
    /// Returns the block producer public key, if any.
    pub fn block_producer(&self) -> Option<fuel_vm::fuel_crypto::PublicKey> {
        let message = self.header.id.clone().into_message();
        match &self.consensus {
            Consensus::PoAConsensus(poa) => {
                let signature = poa.signature.clone().into_signature();
                let producer_pub_key = signature.recover(&message);
                producer_pub_key.ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlockByIdQuery::build(BlockByIdArgs {
            id: BlockId::default(),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn block_mutation_query_gql_output() {
        use cynic::MutationBuilder;
        let operation = BlockMutation::build(ProduceBlockArgs {
            blocks_to_produce: U64(0),
            time: None,
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
