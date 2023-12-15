use graph::blockchain::firehose_block_ingestor::FirehoseBlockIngestor;
use graph::blockchain::BlockIngestor;
use graph::env::EnvVars;
use graph::prelude::MetricsRegistry;
use graph::substreams::Clock;
use std::sync::Arc;

use graph::blockchain::block_stream::{BlockStreamMapper, FirehoseCursor};
use graph::blockchain::client::ChainClient;
use graph::blockchain::{BasicBlockchainBuilder, BlockchainBuilder, NoopRuntimeAdapter};
use graph::cheap_clone::CheapClone;
use graph::components::store::DeploymentCursorTracker;
use graph::data::subgraph::UnifiedMappingApiVersion;
use graph::{
    blockchain::{
        block_stream::{
            BlockStream, BlockStreamEvent, BlockWithTriggers, FirehoseError,
            FirehoseMapper as FirehoseMapperTrait, TriggersAdapter as TriggersAdapterTrait,
        },
        firehose_block_stream::FirehoseBlockStream,
        Block as _, BlockHash, BlockPtr, Blockchain, BlockchainKind, EmptyNodeCapabilities,
        IngestorError, RuntimeAdapter as RuntimeAdapterTrait,
    },
    components::store::DeploymentLocator,
    firehose::{self, FirehoseEndpoint, ForkStep},
    prelude::{async_trait, o, BlockNumber, ChainStore, Error, Logger, LoggerFactory},
};
use prost::Message;

use crate::data_source::{
    DataSource, DataSourceTemplate, EventOrigin, UnresolvedDataSource, UnresolvedDataSourceTemplate,
};
use crate::trigger::CosmosTrigger;
use crate::{codec, TriggerFilter};

pub struct Chain {
    logger_factory: LoggerFactory,
    name: String,
    client: Arc<ChainClient<Self>>,
    chain_store: Arc<dyn ChainStore>,
    metrics_registry: Arc<MetricsRegistry>,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: cosmos")
    }
}

impl BlockchainBuilder<Chain> for BasicBlockchainBuilder {
    fn build(self, _config: &Arc<EnvVars>) -> Chain {
        Chain {
            logger_factory: self.logger_factory,
            name: self.name,
            client: Arc::new(ChainClient::new_firehose(self.firehose_endpoints)),
            chain_store: self.chain_store,
            metrics_registry: self.metrics_registry,
        }
    }
}

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::Cosmos;

    type Client = ();
    type Block = codec::Block;

    type DataSource = DataSource;

    type UnresolvedDataSource = UnresolvedDataSource;

    type DataSourceTemplate = DataSourceTemplate;

    type UnresolvedDataSourceTemplate = UnresolvedDataSourceTemplate;

    type TriggerData = CosmosTrigger;

    type MappingTrigger = CosmosTrigger;

    type TriggerFilter = TriggerFilter;

    type NodeCapabilities = EmptyNodeCapabilities<Self>;

    fn is_refetch_block_required(&self) -> bool {
        false
    }
    async fn refetch_firehose_block(
        &self,
        _logger: &Logger,
        _cursor: FirehoseCursor,
    ) -> Result<codec::Block, Error> {
        unimplemented!("This chain does not support Dynamic Data Sources. is_refetch_block_required always returns false, this shouldn't be called.")
    }

    fn triggers_adapter(
        &self,
        _loc: &DeploymentLocator,
        _capabilities: &Self::NodeCapabilities,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn TriggersAdapterTrait<Self>>, Error> {
        let adapter = TriggersAdapter {};
        Ok(Arc::new(adapter))
    }

    async fn new_block_stream(
        &self,
        deployment: DeploymentLocator,
        store: impl DeploymentCursorTracker,
        start_blocks: Vec<BlockNumber>,
        filter: Arc<Self::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Self>>, Error> {
        let adapter = self
            .triggers_adapter(
                &deployment,
                &EmptyNodeCapabilities::default(),
                unified_api_version,
            )
            .unwrap_or_else(|_| panic!("no adapter for network {}", self.name));

        let logger = self
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "FirehoseBlockStream"));

        let firehose_mapper = Arc::new(FirehoseMapper { adapter, filter });

        Ok(Box::new(FirehoseBlockStream::new(
            deployment.hash,
            self.chain_client(),
            store.block_ptr(),
            store.firehose_cursor(),
            firehose_mapper,
            start_blocks,
            logger,
            self.metrics_registry.clone(),
        )))
    }

    fn chain_store(&self) -> Arc<dyn ChainStore> {
        self.chain_store.cheap_clone()
    }

    async fn block_pointer_from_number(
        &self,
        logger: &Logger,
        number: BlockNumber,
    ) -> Result<BlockPtr, IngestorError> {
        let firehose_endpoint = self.client.firehose_endpoint()?;

        firehose_endpoint
            .block_ptr_for_number::<codec::HeaderOnlyBlock>(logger, number)
            .await
            .map_err(Into::into)
    }

    fn runtime_adapter(&self) -> Arc<dyn RuntimeAdapterTrait<Self>> {
        Arc::new(NoopRuntimeAdapter::default())
    }

    fn chain_client(&self) -> Arc<ChainClient<Self>> {
        self.client.clone()
    }

    fn block_ingestor(&self) -> anyhow::Result<Box<dyn BlockIngestor>> {
        let ingestor = FirehoseBlockIngestor::<crate::Block, Self>::new(
            self.chain_store.cheap_clone(),
            self.chain_client(),
            self.logger_factory
                .component_logger("CosmosFirehoseBlockIngestor", None),
            self.name.clone(),
        );
        Ok(Box::new(ingestor))
    }
}

pub struct TriggersAdapter {}

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter {
    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<codec::Block>, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    async fn scan_triggers(
        &self,
        _from: BlockNumber,
        _to: BlockNumber,
        _filter: &TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<Chain>>, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    async fn triggers_in_block(
        &self,
        logger: &Logger,
        block: codec::Block,
        filter: &TriggerFilter,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        let shared_block = Arc::new(block.clone());

        let header_only_block = codec::HeaderOnlyBlock::from(&block);

        let mut triggers: Vec<_> = shared_block
            .begin_block_events()?
            .cloned()
            // FIXME (Cosmos): Optimize. Should use an Arc instead of cloning the
            // block. This is not currently possible because EventData is automatically
            // generated.
            .filter_map(|event| {
                filter_event_trigger(
                    filter,
                    event,
                    &header_only_block,
                    None,
                    EventOrigin::BeginBlock,
                )
            })
            .chain(shared_block.transactions().flat_map(|tx| {
                tx.result
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .filter_map(|e| {
                        filter_event_trigger(
                            filter,
                            e.clone(),
                            &header_only_block,
                            Some(build_tx_context(tx)),
                            EventOrigin::DeliverTx,
                        )
                    })
                    .collect::<Vec<_>>()
            }))
            .chain(
                shared_block
                    .end_block_events()?
                    .cloned()
                    .filter_map(|event| {
                        filter_event_trigger(
                            filter,
                            event,
                            &header_only_block,
                            None,
                            EventOrigin::EndBlock,
                        )
                    }),
            )
            .collect();

        triggers.extend(shared_block.transactions().cloned().flat_map(|tx_result| {
            let mut triggers: Vec<_> = Vec::new();
            if let Some(tx) = tx_result.tx.clone() {
                if let Some(tx_body) = tx.body {
                    triggers.extend(tx_body.messages.into_iter().map(|message| {
                        CosmosTrigger::with_message(
                            message,
                            header_only_block.clone(),
                            build_tx_context(&tx_result),
                        )
                    }));
                }
            }
            triggers.push(CosmosTrigger::with_transaction(
                tx_result,
                header_only_block.clone(),
            ));
            triggers
        }));

        if filter.block_filter.trigger_every_block {
            triggers.push(CosmosTrigger::Block(shared_block.cheap_clone()));
        }

        Ok(BlockWithTriggers::new(block, triggers, logger))
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    /// Panics if `block` is genesis.
    /// But that's ok since this is only called when reverting `block`.
    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        Ok(Some(BlockPtr {
            hash: BlockHash::from(vec![0xff; 32]),
            number: block.number.saturating_sub(1),
        }))
    }
}

/// Returns a new event trigger only if the given event matches the event filter.
fn filter_event_trigger(
    filter: &TriggerFilter,
    event: codec::Event,
    block: &codec::HeaderOnlyBlock,
    tx_context: Option<codec::TransactionContext>,
    origin: EventOrigin,
) -> Option<CosmosTrigger> {
    if filter.event_type_filter.matches(&event.event_type) {
        Some(CosmosTrigger::with_event(
            event,
            block.clone(),
            tx_context,
            origin,
        ))
    } else {
        None
    }
}

fn build_tx_context(tx: &codec::TxResult) -> codec::TransactionContext {
    codec::TransactionContext {
        hash: tx.hash.clone(),
        index: tx.index,
        code: tx.result.as_ref().unwrap().code,
        gas_wanted: tx.result.as_ref().unwrap().gas_wanted,
        gas_used: tx.result.as_ref().unwrap().gas_used,
    }
}

pub struct FirehoseMapper {
    adapter: Arc<dyn TriggersAdapterTrait<Chain>>,
    filter: Arc<TriggerFilter>,
}

#[async_trait]
impl BlockStreamMapper<Chain> for FirehoseMapper {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<crate::Block>, Error> {
        let block = match output {
            Some(block) => crate::Block::decode(block)?,
            None => anyhow::bail!("cosmos mapper is expected to always have a block"),
        };

        Ok(Some(block))
    }

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: crate::Block,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        self.adapter
            .triggers_in_block(logger, block, self.filter.as_ref())
            .await
    }

    async fn handle_substreams_block(
        &self,
        _logger: &Logger,
        _clock: Clock,
        _cursor: FirehoseCursor,
        _block: Vec<u8>,
    ) -> Result<BlockStreamEvent<Chain>, Error> {
        unimplemented!()
    }
}

#[async_trait]
impl FirehoseMapperTrait<Chain> for FirehoseMapper {
    fn trigger_filter(&self) -> &TriggerFilter {
        self.filter.as_ref()
    }

    async fn to_block_stream_event(
        &self,
        logger: &Logger,
        response: &firehose::Response,
    ) -> Result<BlockStreamEvent<Chain>, FirehoseError> {
        let step = ForkStep::from_i32(response.step).unwrap_or_else(|| {
            panic!(
                "unknown step i32 value {}, maybe you forgot update & re-regenerate the protobuf definitions?",
                response.step
            )
        });

        let any_block = response
            .block
            .as_ref()
            .expect("block payload information should always be present");

        // Right now, this is done in all cases but in reality, with how the BlockStreamEvent::Revert
        // is defined right now, only block hash and block number is necessary. However, this information
        // is not part of the actual bstream::BlockResponseV2 payload. As such, we need to decode the full
        // block which is useless.
        //
        // Check about adding basic information about the block in the bstream::BlockResponseV2 or maybe
        // define a slimmed down struct that would decode only a few fields and ignore all the rest.
        // unwrap: Input cannot be None so output will be error or block.
        let block = self.decode_block(Some(any_block.value.as_ref()))?.unwrap();

        match step {
            ForkStep::StepNew => Ok(BlockStreamEvent::ProcessBlock(
                self.block_with_triggers(logger, block).await?,
                FirehoseCursor::from(response.cursor.clone()),
            )),

            ForkStep::StepUndo => {
                let parent_ptr = block
                    .parent_ptr()
                    .map_err(FirehoseError::from)?
                    .expect("Genesis block should never be reverted");

                Ok(BlockStreamEvent::Revert(
                    parent_ptr,
                    FirehoseCursor::from(response.cursor.clone()),
                ))
            }

            ForkStep::StepFinal => {
                panic!(
                    "final step is not handled and should not be requested in the Firehose request"
                )
            }

            ForkStep::StepUnset => {
                panic!("unknown step should not happen in the Firehose response")
            }
        }
    }

    async fn block_ptr_for_number(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        number: BlockNumber,
    ) -> Result<BlockPtr, Error> {
        endpoint
            .block_ptr_for_number::<codec::HeaderOnlyBlock>(logger, number)
            .await
    }

    async fn final_block_ptr_for(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        block: &codec::Block,
    ) -> Result<BlockPtr, Error> {
        // Cosmos provides instant block finality.
        self.block_ptr_for_number(logger, endpoint, block.number())
            .await
    }
}

#[cfg(test)]
mod test {
    use graph::prelude::{
        slog::{o, Discard, Logger},
        tokio,
    };

    use super::*;

    use codec::{
        Block, Event, Header, HeaderOnlyBlock, ResponseBeginBlock, ResponseDeliverTx,
        ResponseEndBlock, TxResult,
    };

    #[tokio::test]
    async fn test_trigger_filters() {
        let adapter = TriggersAdapter {};
        let logger = Logger::root(Discard, o!());

        let block_with_events = Block::test_with_event_types(
            vec!["begin_event_1", "begin_event_2", "begin_event_3"],
            vec!["tx_event_1", "tx_event_2", "tx_event_3"],
            vec!["end_event_1", "end_event_2", "end_event_3"],
        );

        let header_only_block = HeaderOnlyBlock::from(&block_with_events);

        let cases = [
            (
                Block::test_new(),
                TriggerFilter::test_new(false, &[]),
                vec![],
            ),
            (
                Block::test_new(),
                TriggerFilter::test_new(true, &[]),
                vec![CosmosTrigger::Block(Arc::new(Block::test_new()))],
            ),
            (
                Block::test_new(),
                TriggerFilter::test_new(false, &["event_1", "event_2", "event_3"]),
                vec![],
            ),
            (
                block_with_events.clone(),
                TriggerFilter::test_new(false, &["begin_event_3", "tx_event_3", "end_event_3"]),
                vec![
                    CosmosTrigger::with_event(
                        Event::test_with_type("begin_event_3"),
                        header_only_block.clone(),
                        None,
                        EventOrigin::BeginBlock,
                    ),
                    CosmosTrigger::with_event(
                        Event::test_with_type("tx_event_3"),
                        header_only_block.clone(),
                        Some(build_tx_context(&block_with_events.transactions[2])),
                        EventOrigin::DeliverTx,
                    ),
                    CosmosTrigger::with_event(
                        Event::test_with_type("end_event_3"),
                        header_only_block.clone(),
                        None,
                        EventOrigin::EndBlock,
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_1"),
                        header_only_block.clone(),
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_2"),
                        header_only_block.clone(),
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_3"),
                        header_only_block.clone(),
                    ),
                ],
            ),
            (
                block_with_events.clone(),
                TriggerFilter::test_new(true, &["begin_event_3", "tx_event_2", "end_event_1"]),
                vec![
                    CosmosTrigger::Block(Arc::new(block_with_events.clone())),
                    CosmosTrigger::with_event(
                        Event::test_with_type("begin_event_3"),
                        header_only_block.clone(),
                        None,
                        EventOrigin::BeginBlock,
                    ),
                    CosmosTrigger::with_event(
                        Event::test_with_type("tx_event_2"),
                        header_only_block.clone(),
                        Some(build_tx_context(&block_with_events.transactions[1])),
                        EventOrigin::DeliverTx,
                    ),
                    CosmosTrigger::with_event(
                        Event::test_with_type("end_event_1"),
                        header_only_block.clone(),
                        None,
                        EventOrigin::EndBlock,
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_1"),
                        header_only_block.clone(),
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_2"),
                        header_only_block.clone(),
                    ),
                    CosmosTrigger::with_transaction(
                        TxResult::test_with_event_type("tx_event_3"),
                        header_only_block.clone(),
                    ),
                ],
            ),
        ];

        for (block, trigger_filter, expected_triggers) in cases {
            let triggers = adapter
                .triggers_in_block(&logger, block, &trigger_filter)
                .await
                .expect("failed to get triggers in block");

            assert_eq!(
                triggers.trigger_data.len(),
                expected_triggers.len(),
                "Expected trigger list to contain exactly {:?}, but it didn't: {:?}",
                expected_triggers,
                triggers.trigger_data
            );

            // they may not be in the same order
            for trigger in expected_triggers {
                assert!(
                    triggers.trigger_data.contains(&trigger),
                    "Expected trigger list to contain {:?}, but it only contains: {:?}",
                    trigger,
                    triggers.trigger_data
                );
            }
        }
    }

    impl Block {
        fn test_new() -> Block {
            Block::test_with_event_types(vec![], vec![], vec![])
        }

        fn test_with_event_types(
            begin_event_types: Vec<&str>,
            tx_event_types: Vec<&str>,
            end_event_types: Vec<&str>,
        ) -> Block {
            Block {
                header: Some(Header {
                    version: None,
                    chain_id: "test".to_string(),
                    height: 1,
                    time: None,
                    last_block_id: None,
                    last_commit_hash: vec![],
                    data_hash: vec![],
                    validators_hash: vec![],
                    next_validators_hash: vec![],
                    consensus_hash: vec![],
                    app_hash: vec![],
                    last_results_hash: vec![],
                    evidence_hash: vec![],
                    proposer_address: vec![],
                    hash: vec![],
                }),
                evidence: None,
                last_commit: None,
                result_begin_block: Some(ResponseBeginBlock {
                    events: begin_event_types
                        .into_iter()
                        .map(Event::test_with_type)
                        .collect(),
                }),
                result_end_block: Some(ResponseEndBlock {
                    validator_updates: vec![],
                    consensus_param_updates: None,
                    events: end_event_types
                        .into_iter()
                        .map(Event::test_with_type)
                        .collect(),
                }),
                transactions: tx_event_types
                    .into_iter()
                    .map(TxResult::test_with_event_type)
                    .collect(),
                validator_updates: vec![],
            }
        }
    }

    impl Event {
        fn test_with_type(event_type: &str) -> Event {
            Event {
                event_type: event_type.to_string(),
                attributes: vec![],
            }
        }
    }

    impl TxResult {
        fn test_with_event_type(event_type: &str) -> TxResult {
            TxResult {
                height: 1,
                index: 1,
                tx: None,
                result: Some(ResponseDeliverTx {
                    code: 1,
                    data: vec![],
                    log: "".to_string(),
                    info: "".to_string(),
                    gas_wanted: 1,
                    gas_used: 1,
                    codespace: "".to_string(),
                    events: vec![Event::test_with_type(event_type)],
                }),
                hash: vec![],
            }
        }
    }
}
