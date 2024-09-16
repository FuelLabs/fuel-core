use super::Bytes32;
use crate::client::schema::{
    schema,
    BlockId,
    ConnectionArgs,
    PageInfo,
    Signature,
    Tai64Timestamp,
    TransactionId,
    U16,
    U32,
    U64,
};
use fuel_core_types::{
    fuel_crypto,
    fuel_types::BlockHeight,
};

#[derive(cynic::QueryVariables, Debug)]
pub struct BlockByIdArgs {
    pub id: Option<BlockId>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "BlockByIdArgs"
)]
pub struct BlockByIdQuery {
    #[arguments(id: $id)]
    pub block: Option<Block>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct BlockByHeightArgs {
    pub height: Option<U32>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "BlockByHeightArgs"
)]
pub struct BlockByHeightQuery {
    #[arguments(height: $height)]
    pub block: Option<Block>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ConnectionArgs"
)]
pub struct BlocksQuery {
    #[arguments(after: $after, before: $before, first: $first, last: $last)]
    pub blocks: BlockConnection,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct BlockConnection {
    pub edges: Vec<BlockEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct BlockEdge {
    pub cursor: String,
    pub node: Block,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum BlockVersion {
    V1,
}

/// Block with transaction ids
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Block {
    pub version: BlockVersion,
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transaction_ids: Vec<TransactionId>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Block")]
pub struct BlockIdFragment {
    pub id: BlockId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Block")]
pub struct BlockHeightFragment {
    pub height: U32,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ProduceBlockArgs {
    pub start_timestamp: Option<Tai64Timestamp>,
    pub blocks_to_produce: U32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    variables = "ProduceBlockArgs",
    graphql_type = "Mutation"
)]
pub struct BlockMutation {
    #[arguments(blocksToProduce: $blocks_to_produce, startTimestamp: $start_timestamp)]
    pub produce_blocks: U32,
}

#[derive(cynic::Enum, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum HeaderVersion {
    V1,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Header {
    pub version: HeaderVersion,
    pub id: BlockId,
    pub da_height: U64,
    pub consensus_parameters_version: U32,
    pub state_transition_bytecode_version: U32,
    pub transactions_count: U16,
    pub message_receipt_count: U32,
    pub transactions_root: Bytes32,
    pub message_outbox_root: Bytes32,
    pub event_inbox_root: Bytes32,
    pub height: U32,
    pub prev_root: Bytes32,
    pub time: Tai64Timestamp,
    pub application_hash: Bytes32,
}

#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Consensus {
    Genesis(Genesis),
    PoAConsensus(PoAConsensus),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Genesis {
    pub chain_config_hash: Bytes32,
    pub coins_root: Bytes32,
    pub contracts_root: Bytes32,
    pub messages_root: Bytes32,
    pub transactions_root: Bytes32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PoAConsensus {
    pub signature: Signature,
}

impl Block {
    /// Returns the block producer public key, if any.
    pub fn block_producer(&self) -> Option<fuel_crypto::PublicKey> {
        let message = self.header.id.clone().into_message();
        match &self.consensus {
            Consensus::Genesis(_) => Some(Default::default()),
            Consensus::PoAConsensus(poa) => {
                let signature = poa.signature.clone().into_signature();
                let producer_pub_key = signature.recover(&message);
                producer_pub_key.ok()
            }
            Consensus::Unknown => None,
        }
    }
}

impl From<BlockHeightFragment> for BlockHeight {
    fn from(fragment: BlockHeightFragment) -> Self {
        BlockHeight::new(fragment.height.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlockByIdQuery::build(BlockByIdArgs {
            id: Some(BlockId::default()),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn block_by_height_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = BlockByHeightQuery::build(BlockByHeightArgs {
            height: Some(U32(0)),
        });
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn block_mutation_query_gql_output() {
        use cynic::MutationBuilder;
        let operation = BlockMutation::build(ProduceBlockArgs {
            blocks_to_produce: U32(0),
            start_timestamp: None,
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
