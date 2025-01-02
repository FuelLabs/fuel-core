#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(warnings)]

// TODO: This was copy pasted from

use cynic::QueryBuilder;
use fuel_core_client::{
    client,
    client::{
        pagination::{
            PaginatedResult,
            PaginationRequest,
        },
        schema::{
            block::{
                BlockByHeightArgs,
                Consensus,
                Header,
            },
            schema,
            tx::OpaqueTransactionWithStatus,
            ConnectionArgs,
            PageInfo,
        },
        types::{
            TransactionResponse,
            TransactionStatus,
        },
        FuelClient,
    },
};
use fuel_core_types::{
    blockchain::{
        self,
        block::Block,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        SealedBlock,
    },
    fuel_tx::{
        Bytes32,
        Receipt,
    },
};
use itertools::Itertools;

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./target/schema.sdl",
    graphql_type = "Query",
    variables = "ConnectionArgs"
)]
pub struct FullBlocksQuery {
    #[arguments(after: $after, before: $before, first: $first, last: $last)]
    pub blocks: FullBlockConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./target/schema.sdl", graphql_type = "BlockConnection")]
pub struct FullBlockConnection {
    pub edges: Vec<FullBlockEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./target/schema.sdl", graphql_type = "BlockEdge")]
pub struct FullBlockEdge {
    pub cursor: String,
    pub node: FullBlock,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./target/schema.sdl",
    graphql_type = "Query",
    variables = "BlockByHeightArgs"
)]
pub struct FullBlockByHeightQuery {
    #[arguments(height: $height)]
    pub block: Option<FullBlock>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./target/schema.sdl", graphql_type = "Block")]
pub struct FullBlock {
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<OpaqueTransactionWithStatus>,
}

impl From<FullBlockConnection> for PaginatedResult<FullBlock, String> {
    fn from(conn: FullBlockConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node).collect(),
        }
    }
}

#[async_trait::async_trait]
pub trait ClientExt {
    async fn full_blocks(
        &self,
        request: PaginationRequest<String>,
    ) -> std::io::Result<PaginatedResult<FullBlock, String>>;
}

#[async_trait::async_trait]
impl ClientExt for FuelClient {
    async fn full_blocks(
        &self,
        request: PaginationRequest<String>,
    ) -> std::io::Result<PaginatedResult<FullBlock, String>> {
        let query = FullBlocksQuery::build(request.into());
        let blocks = self.query(query).await?.blocks.into();
        Ok(blocks)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedBlockWithMetadata {
    pub block: SealedBlock,
    pub receipts: Vec<Option<Vec<Receipt>>>,
}

impl TryFrom<FullBlock> for SealedBlockWithMetadata {
    type Error = anyhow::Error;

    fn try_from(full_block: FullBlock) -> Result<Self, Self::Error> {
        let transactions: Vec<TransactionResponse> = full_block
            .transactions
            .into_iter()
            .map(TryInto::try_into)
            .try_collect()?;

        let receipts = transactions
            .iter()
            .map(|tx| &tx.status)
            .map(|status| match status {
                TransactionStatus::Success { receipts, .. } => Some(receipts.clone()),
                _ => None,
            })
            .collect_vec();

        let messages = receipts
            .iter()
            .flatten()
            .flat_map(|receipt| receipt.iter().filter_map(|r| r.message_id()))
            .collect_vec();

        let transactions = transactions
            .into_iter()
            .map(|tx| tx.transaction)
            .collect_vec();

        let partial_header = PartialBlockHeader {
            application: ApplicationHeader {
                da_height: full_block.header.da_height.0.into(),
                consensus_parameters_version: full_block
                    .header
                    .consensus_parameters_version
                    .into(),
                state_transition_bytecode_version: full_block
                    .header
                    .state_transition_bytecode_version
                    .into(),
                generated: Default::default(),
            },
            consensus: ConsensusHeader {
                prev_root: full_block.header.prev_root.into(),
                height: full_block.header.height.into(),
                time: full_block.header.time.into(),
                generated: Default::default(),
            },
        };

        let header = partial_header
            .generate(
                &transactions,
                &messages,
                full_block.header.event_inbox_root.into(),
            )
            .map_err(|e| anyhow::anyhow!(e))?;

        let actual_id: Bytes32 = full_block.header.id.into();
        let expected_id: Bytes32 = header.id().into();
        if expected_id != actual_id {
            return Err(anyhow::anyhow!("Header id mismatch"));
        }

        let block = Block::try_from_executed(header, transactions)
            .ok_or(anyhow::anyhow!("Failed to create block from transactions"))?;

        let consensus: client::types::Consensus = full_block.consensus.into();

        let consensus = match consensus {
            client::types::Consensus::Genesis(genesis) => {
                use blockchain::consensus as core_consensus;
                core_consensus::Consensus::Genesis(core_consensus::Genesis {
                    chain_config_hash: genesis.chain_config_hash,
                    coins_root: genesis.coins_root,
                    contracts_root: genesis.contracts_root,
                    messages_root: genesis.messages_root,
                    transactions_root: genesis.transactions_root,
                })
            }
            client::types::Consensus::PoAConsensus(poa) => {
                use blockchain::consensus as core_consensus;
                core_consensus::Consensus::PoA(core_consensus::poa::PoAConsensus {
                    signature: poa.signature,
                })
            }
            client::types::Consensus::Unknown => {
                return Err(anyhow::anyhow!("Unknown consensus type"));
            }
        };

        let sealed = SealedBlock {
            entity: block,
            consensus,
        };

        let sealed = SealedBlockWithMetadata {
            block: sealed,
            receipts,
        };

        Ok(sealed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_client::client::pagination::PageDirection;

    #[tokio::test]
    async fn testnet_works() {
        let client = FuelClient::new("https://testnet.fuel.network")
            .expect("Should connect to the beta 5 network");

        let request = PaginationRequest {
            cursor: None,
            results: 1,
            direction: PageDirection::Backward,
        };
        let full_blocks = client
            .full_blocks(request)
            .await
            .expect("Should get a blocks");

        let full_block = full_blocks
            .results
            .into_iter()
            .next()
            .expect("Should have a block");
        let result: anyhow::Result<SealedBlockWithMetadata> = full_block.try_into();
        assert!(result.is_ok(), "{result:?}");
    }
}
