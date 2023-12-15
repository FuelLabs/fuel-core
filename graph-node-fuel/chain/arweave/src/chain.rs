use graph::anyhow;
use graph::blockchain::client::ChainClient;
use graph::blockchain::firehose_block_ingestor::FirehoseBlockIngestor;
use graph::blockchain::{
    BasicBlockchainBuilder, Block, BlockIngestor, BlockchainBuilder, BlockchainKind,
    EmptyNodeCapabilities, NoopRuntimeAdapter,
};
use graph::cheap_clone::CheapClone;
use graph::components::store::DeploymentCursorTracker;
use graph::data::subgraph::UnifiedMappingApiVersion;
use graph::env::EnvVars;
use graph::firehose::FirehoseEndpoint;
use graph::prelude::MetricsRegistry;
use graph::substreams::Clock;
use graph::{
    blockchain::{
        block_stream::{
            BlockStreamEvent, BlockWithTriggers, FirehoseError,
            FirehoseMapper as FirehoseMapperTrait, TriggersAdapter as TriggersAdapterTrait,
        },
        firehose_block_stream::FirehoseBlockStream,
        BlockHash, BlockPtr, Blockchain, IngestorError, RuntimeAdapter as RuntimeAdapterTrait,
    },
    components::store::DeploymentLocator,
    firehose::{self as firehose, ForkStep},
    prelude::{async_trait, o, BlockNumber, ChainStore, Error, Logger, LoggerFactory},
};
use prost::Message;
use std::sync::Arc;

use crate::adapter::TriggerFilter;
use crate::data_source::{DataSourceTemplate, UnresolvedDataSourceTemplate};
use crate::trigger::{self, ArweaveTrigger};
use crate::{
    codec,
    data_source::{DataSource, UnresolvedDataSource},
};
use graph::blockchain::block_stream::{BlockStream, BlockStreamMapper, FirehoseCursor};

pub struct Chain {
    logger_factory: LoggerFactory,
    name: String,
    client: Arc<ChainClient<Self>>,
    chain_store: Arc<dyn ChainStore>,
    metrics_registry: Arc<MetricsRegistry>,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: arweave")
    }
}

impl BlockchainBuilder<Chain> for BasicBlockchainBuilder {
    fn build(self, _config: &Arc<EnvVars>) -> Chain {
        Chain {
            logger_factory: self.logger_factory,
            name: self.name,
            client: Arc::new(ChainClient::<Chain>::new_firehose(self.firehose_endpoints)),
            chain_store: self.chain_store,
            metrics_registry: self.metrics_registry,
        }
    }
}

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::Arweave;

    type Client = ();
    type Block = codec::Block;

    type DataSource = DataSource;

    type UnresolvedDataSource = UnresolvedDataSource;

    type DataSourceTemplate = DataSourceTemplate;

    type UnresolvedDataSourceTemplate = UnresolvedDataSourceTemplate;

    type TriggerData = crate::trigger::ArweaveTrigger;

    type MappingTrigger = crate::trigger::ArweaveTrigger;

    type TriggerFilter = crate::adapter::TriggerFilter;

    type NodeCapabilities = EmptyNodeCapabilities<Self>;

    fn triggers_adapter(
        &self,
        _loc: &DeploymentLocator,
        _capabilities: &Self::NodeCapabilities,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn TriggersAdapterTrait<Self>>, Error> {
        let adapter = TriggersAdapter {};
        Ok(Arc::new(adapter))
    }

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
        self.chain_store.clone()
    }

    async fn block_pointer_from_number(
        &self,
        logger: &Logger,
        number: BlockNumber,
    ) -> Result<BlockPtr, IngestorError> {
        self.client
            .firehose_endpoint()?
            .block_ptr_for_number::<codec::Block>(logger, number)
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
                .component_logger("ArweaveFirehoseBlockIngestor", None),
            self.name.clone(),
        );
        Ok(Box::new(ingestor))
    }
}

pub struct TriggersAdapter {}

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter {
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
        // TODO: Find the best place to introduce an `Arc` and avoid this clone.
        let shared_block = Arc::new(block.clone());

        let TriggerFilter {
            block_filter,
            transaction_filter,
        } = filter;

        let txs = block
            .clone()
            .txs
            .into_iter()
            .filter(|tx| transaction_filter.matches(&tx.owner))
            .map(|tx| trigger::TransactionWithBlockPtr {
                tx: Arc::new(tx),
                block: shared_block.clone(),
            })
            .collect::<Vec<_>>();

        let mut trigger_data: Vec<_> = txs
            .into_iter()
            .map(|tx| ArweaveTrigger::Transaction(Arc::new(tx)))
            .collect();

        if block_filter.trigger_every_block {
            trigger_data.push(ArweaveTrigger::Block(shared_block.cheap_clone()));
        }

        Ok(BlockWithTriggers::new(block, trigger_data, logger))
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<codec::Block>, Error> {
        panic!("Should never be called since FirehoseBlockStream cannot resolve it")
    }

    /// Panics if `block` is genesis.
    /// But that's ok since this is only called when reverting `block`.
    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        // FIXME (Arweave):  Might not be necessary for Arweave support for now
        Ok(Some(BlockPtr {
            hash: BlockHash::from(vec![0xff; 48]),
            number: block.number.saturating_sub(1),
        }))
    }
}

pub struct FirehoseMapper {
    adapter: Arc<dyn TriggersAdapterTrait<Chain>>,
    filter: Arc<TriggerFilter>,
}

#[async_trait]
impl BlockStreamMapper<Chain> for FirehoseMapper {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<codec::Block>, Error> {
        let block = match output {
            Some(block) => codec::Block::decode(block)?,
            None => anyhow::bail!("Arweave mapper is expected to always have a block"),
        };

        Ok(Some(block))
    }

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: codec::Block,
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
        // define a slimmed down stuct that would decode only a few fields and ignore all the rest.
        // unwrap: Input cannot be None so output will be error or block.
        let block = self.decode_block(Some(&any_block.value.as_ref()))?.unwrap();

        use ForkStep::*;
        match step {
            StepNew => Ok(BlockStreamEvent::ProcessBlock(
                self.block_with_triggers(&logger, block).await?,
                FirehoseCursor::from(response.cursor.clone()),
            )),

            StepUndo => {
                let parent_ptr = block
                    .parent_ptr()
                    .expect("Genesis block should never be reverted");

                Ok(BlockStreamEvent::Revert(
                    parent_ptr,
                    FirehoseCursor::from(response.cursor.clone()),
                ))
            }

            StepFinal => {
                panic!("irreversible step is not handled and should not be requested in the Firehose request")
            }

            StepUnset => {
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
            .block_ptr_for_number::<codec::Block>(logger, number)
            .await
    }

    // # FIXME
    //
    // the final block of arweave is itself in the current implementation
    async fn final_block_ptr_for(
        &self,
        _logger: &Logger,
        _endpoint: &Arc<FirehoseEndpoint>,
        block: &codec::Block,
    ) -> Result<BlockPtr, Error> {
        Ok(block.ptr())
    }
}
