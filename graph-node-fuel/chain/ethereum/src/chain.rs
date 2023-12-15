use anyhow::{anyhow, bail, Result};
use anyhow::{Context, Error};
use graph::blockchain::client::ChainClient;
use graph::blockchain::firehose_block_ingestor::{FirehoseBlockIngestor, Transforms};
use graph::blockchain::{BlockIngestor, BlockchainKind, TriggersAdapterSelector};
use graph::components::store::DeploymentCursorTracker;
use graph::data::subgraph::UnifiedMappingApiVersion;
use graph::firehose::{FirehoseEndpoint, ForkStep};
use graph::prelude::{
    BlockHash, ComponentLoggerConfig, ElasticComponentLoggerConfig, EthereumBlock,
    EthereumCallCache, LightEthereumBlock, LightEthereumBlockExt, MetricsRegistry,
};
use graph::schema::InputSchema;
use graph::substreams::Clock;
use graph::{
    blockchain::{
        block_stream::{
            BlockRefetcher, BlockStreamEvent, BlockWithTriggers, FirehoseError,
            FirehoseMapper as FirehoseMapperTrait, TriggersAdapter as TriggersAdapterTrait,
        },
        firehose_block_stream::FirehoseBlockStream,
        polling_block_stream::PollingBlockStream,
        Block, BlockPtr, Blockchain, ChainHeadUpdateListener, IngestorError,
        RuntimeAdapter as RuntimeAdapterTrait, TriggerFilter as _,
    },
    cheap_clone::CheapClone,
    components::store::DeploymentLocator,
    firehose,
    prelude::{
        async_trait, o, serde_json as json, BlockNumber, ChainStore, EthereumBlockWithCalls,
        Future01CompatExt, Logger, LoggerFactory, NodeId,
    },
};
use prost::Message;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::sync::Arc;
use std::time::Duration;

use crate::codec::HeaderOnlyBlock;
use crate::data_source::DataSourceTemplate;
use crate::data_source::UnresolvedDataSourceTemplate;
use crate::ingestor::PollingBlockIngestor;
use crate::network::EthereumNetworkAdapters;
use crate::EthereumAdapter;
use crate::NodeCapabilities;
use crate::{
    adapter::EthereumAdapter as _,
    codec,
    data_source::{DataSource, UnresolvedDataSource},
    ethereum_adapter::{
        blocks_with_triggers, get_calls, parse_block_triggers, parse_call_triggers,
        parse_log_triggers,
    },
    SubgraphEthRpcMetrics, TriggerFilter, ENV_VARS,
};
use graph::blockchain::block_stream::{
    BlockStream, BlockStreamBuilder, BlockStreamMapper, FirehoseCursor,
};

/// Celo Mainnet: 42220, Testnet Alfajores: 44787, Testnet Baklava: 62320
const CELO_CHAIN_IDS: [u64; 3] = [42220, 44787, 62320];

pub struct EthereumStreamBuilder {}

#[async_trait]
impl BlockStreamBuilder<Chain> for EthereumStreamBuilder {
    async fn build_firehose(
        &self,
        chain: &Chain,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<Chain as Blockchain>::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        let requirements = filter.node_capabilities();
        let adapter = chain
            .triggers_adapter(&deployment, &requirements, unified_api_version)
            .unwrap_or_else(|_| {
                panic!(
                    "no adapter for network {} with capabilities {}",
                    chain.name, requirements
                )
            });

        let logger = chain
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "FirehoseBlockStream"));

        let firehose_mapper = Arc::new(FirehoseMapper { adapter, filter });

        Ok(Box::new(FirehoseBlockStream::new(
            deployment.hash,
            chain.chain_client(),
            subgraph_current_block,
            block_cursor,
            firehose_mapper,
            start_blocks,
            logger,
            chain.registry.clone(),
        )))
    }

    async fn build_substreams(
        &self,
        _chain: &Chain,
        _schema: InputSchema,
        _deployment: DeploymentLocator,
        _block_cursor: FirehoseCursor,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<<Chain as Blockchain>::TriggerFilter>,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        unimplemented!()
    }

    async fn build_polling(
        &self,
        chain: &Chain,
        deployment: DeploymentLocator,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<Chain as Blockchain>::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        let requirements = filter.node_capabilities();
        let adapter = chain
            .triggers_adapter(&deployment, &requirements, unified_api_version.clone())
            .unwrap_or_else(|_| {
                panic!(
                    "no adapter for network {} with capabilities {}",
                    chain.name, requirements
                )
            });

        let logger = chain
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "BlockStream"));
        let chain_store = chain.chain_store();
        let chain_head_update_stream = chain
            .chain_head_update_listener
            .subscribe(chain.name.clone(), logger.clone());

        // Special case: Detect Celo and set the threshold to 0, so that eth_getLogs is always used.
        // This is ok because Celo blocks are always final. And we _need_ to do this because
        // some events appear only in eth_getLogs but not in transaction receipts.
        // See also ca0edc58-0ec5-4c89-a7dd-2241797f5e50.
        let chain_id = match chain.chain_client().as_ref() {
            ChainClient::Rpc(adapter) => {
                adapter
                    .cheapest()
                    .ok_or(anyhow!("unable to get eth adapter for chan_id call"))?
                    .chain_id()
                    .await?
            }
            _ => panic!("expected rpc when using polling blockstream"),
        };
        let reorg_threshold = match CELO_CHAIN_IDS.contains(&chain_id) {
            false => chain.reorg_threshold,
            true => 0,
        };

        Ok(Box::new(PollingBlockStream::new(
            chain_store,
            chain_head_update_stream,
            adapter,
            chain.node_id.clone(),
            deployment.hash,
            filter,
            start_blocks,
            reorg_threshold,
            logger,
            ENV_VARS.max_block_range_size,
            ENV_VARS.target_triggers_per_block_range,
            unified_api_version,
            subgraph_current_block,
        )))
    }
}

pub struct EthereumBlockRefetcher {}

#[async_trait]
impl BlockRefetcher<Chain> for EthereumBlockRefetcher {
    fn required(&self, chain: &Chain) -> bool {
        chain.chain_client().is_firehose()
    }

    async fn get_block(
        &self,
        chain: &Chain,
        logger: &Logger,
        cursor: FirehoseCursor,
    ) -> Result<BlockFinality, Error> {
        let endpoint = chain.chain_client().firehose_endpoint()?;
        let block = endpoint.get_block::<codec::Block>(cursor, logger).await?;
        let ethereum_block: EthereumBlockWithCalls = (&block).try_into()?;
        Ok(BlockFinality::NonFinal(ethereum_block))
    }
}

pub struct EthereumAdapterSelector {
    logger_factory: LoggerFactory,
    client: Arc<ChainClient<Chain>>,
    registry: Arc<MetricsRegistry>,
    chain_store: Arc<dyn ChainStore>,
}

impl EthereumAdapterSelector {
    pub fn new(
        logger_factory: LoggerFactory,
        client: Arc<ChainClient<Chain>>,
        registry: Arc<MetricsRegistry>,
        chain_store: Arc<dyn ChainStore>,
    ) -> Self {
        Self {
            logger_factory,
            client,
            registry,
            chain_store,
        }
    }
}

impl TriggersAdapterSelector<Chain> for EthereumAdapterSelector {
    fn triggers_adapter(
        &self,
        loc: &DeploymentLocator,
        capabilities: &<Chain as Blockchain>::NodeCapabilities,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn TriggersAdapterTrait<Chain>>, Error> {
        let logger = self
            .logger_factory
            .subgraph_logger(loc)
            .new(o!("component" => "BlockStream"));

        let ethrpc_metrics = Arc::new(SubgraphEthRpcMetrics::new(self.registry.clone(), &loc.hash));

        let adapter = TriggersAdapter {
            logger: logger.clone(),
            ethrpc_metrics,
            chain_client: self.client.cheap_clone(),
            chain_store: self.chain_store.cheap_clone(),
            unified_api_version,
            capabilities: *capabilities,
        };
        Ok(Arc::new(adapter))
    }
}

pub struct Chain {
    logger_factory: LoggerFactory,
    name: String,
    node_id: NodeId,
    registry: Arc<MetricsRegistry>,
    client: Arc<ChainClient<Self>>,
    chain_store: Arc<dyn ChainStore>,
    call_cache: Arc<dyn EthereumCallCache>,
    chain_head_update_listener: Arc<dyn ChainHeadUpdateListener>,
    reorg_threshold: BlockNumber,
    polling_ingestor_interval: Duration,
    pub is_ingestible: bool,
    block_stream_builder: Arc<dyn BlockStreamBuilder<Self>>,
    block_refetcher: Arc<dyn BlockRefetcher<Self>>,
    adapter_selector: Arc<dyn TriggersAdapterSelector<Self>>,
    runtime_adapter: Arc<dyn RuntimeAdapterTrait<Self>>,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: ethereum")
    }
}

impl Chain {
    /// Creates a new Ethereum [`Chain`].
    pub fn new(
        logger_factory: LoggerFactory,
        name: String,
        node_id: NodeId,
        registry: Arc<MetricsRegistry>,
        chain_store: Arc<dyn ChainStore>,
        call_cache: Arc<dyn EthereumCallCache>,
        client: Arc<ChainClient<Self>>,
        chain_head_update_listener: Arc<dyn ChainHeadUpdateListener>,
        block_stream_builder: Arc<dyn BlockStreamBuilder<Self>>,
        block_refetcher: Arc<dyn BlockRefetcher<Self>>,
        adapter_selector: Arc<dyn TriggersAdapterSelector<Self>>,
        runtime_adapter: Arc<dyn RuntimeAdapterTrait<Self>>,
        reorg_threshold: BlockNumber,
        polling_ingestor_interval: Duration,
        is_ingestible: bool,
    ) -> Self {
        Chain {
            logger_factory,
            name,
            node_id,
            registry,
            client,
            chain_store,
            call_cache,
            chain_head_update_listener,
            block_stream_builder,
            block_refetcher,
            adapter_selector,
            runtime_adapter,
            reorg_threshold,
            is_ingestible,
            polling_ingestor_interval,
        }
    }

    /// Returns a handler to this chain's [`EthereumCallCache`].
    pub fn call_cache(&self) -> Arc<dyn EthereumCallCache> {
        self.call_cache.clone()
    }

    // TODO: This is only used to build the block stream which could prolly
    // be moved to the chain itself and return a block stream future that the
    // caller can spawn.
    pub fn cheapest_adapter(&self) -> Arc<EthereumAdapter> {
        let adapters = match self.client.as_ref() {
            ChainClient::Firehose(_) => panic!("no adapter with firehose"),
            ChainClient::Rpc(adapter) => adapter,
        };
        adapters.cheapest().unwrap()
    }
}

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::Ethereum;
    const ALIASES: &'static [&'static str] = &["ethereum/contract"];

    type Client = EthereumNetworkAdapters;
    type Block = BlockFinality;

    type DataSource = DataSource;

    type UnresolvedDataSource = UnresolvedDataSource;

    type DataSourceTemplate = DataSourceTemplate;

    type UnresolvedDataSourceTemplate = UnresolvedDataSourceTemplate;

    type TriggerData = crate::trigger::EthereumTrigger;

    type MappingTrigger = crate::trigger::MappingTrigger;

    type TriggerFilter = crate::adapter::TriggerFilter;

    type NodeCapabilities = crate::capabilities::NodeCapabilities;

    fn triggers_adapter(
        &self,
        loc: &DeploymentLocator,
        capabilities: &Self::NodeCapabilities,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn TriggersAdapterTrait<Self>>, Error> {
        self.adapter_selector
            .triggers_adapter(loc, capabilities, unified_api_version)
    }

    async fn new_block_stream(
        &self,
        deployment: DeploymentLocator,
        store: impl DeploymentCursorTracker,
        start_blocks: Vec<BlockNumber>,
        filter: Arc<Self::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Self>>, Error> {
        let current_ptr = store.block_ptr();
        match self.chain_client().as_ref() {
            ChainClient::Rpc(_) => {
                self.block_stream_builder
                    .build_polling(
                        self,
                        deployment,
                        start_blocks,
                        current_ptr,
                        filter,
                        unified_api_version,
                    )
                    .await
            }
            ChainClient::Firehose(_) => {
                self.block_stream_builder
                    .build_firehose(
                        self,
                        deployment,
                        store.firehose_cursor(),
                        start_blocks,
                        current_ptr,
                        filter,
                        unified_api_version,
                    )
                    .await
            }
        }
    }

    fn chain_store(&self) -> Arc<dyn ChainStore> {
        self.chain_store.clone()
    }

    async fn block_pointer_from_number(
        &self,
        logger: &Logger,
        number: BlockNumber,
    ) -> Result<BlockPtr, IngestorError> {
        match self.client.as_ref() {
            ChainClient::Firehose(endpoints) => endpoints
                .endpoint()?
                .block_ptr_for_number::<HeaderOnlyBlock>(logger, number)
                .await
                .map_err(IngestorError::Unknown),
            ChainClient::Rpc(adapters) => {
                let adapter = adapters
                    .cheapest()
                    .with_context(|| format!("no adapter for chain {}", self.name))?
                    .clone();

                adapter
                    .block_pointer_from_number(logger, number)
                    .compat()
                    .await
            }
        }
    }

    fn is_refetch_block_required(&self) -> bool {
        self.block_refetcher.required(self)
    }

    async fn refetch_firehose_block(
        &self,
        logger: &Logger,
        cursor: FirehoseCursor,
    ) -> Result<BlockFinality, Error> {
        self.block_refetcher.get_block(self, logger, cursor).await
    }

    fn runtime_adapter(&self) -> Arc<dyn RuntimeAdapterTrait<Self>> {
        self.runtime_adapter.clone()
    }

    fn chain_client(&self) -> Arc<ChainClient<Self>> {
        self.client.clone()
    }

    fn block_ingestor(&self) -> anyhow::Result<Box<dyn BlockIngestor>> {
        let ingestor: Box<dyn BlockIngestor> = match self.chain_client().as_ref() {
            ChainClient::Firehose(_) => {
                let ingestor = FirehoseBlockIngestor::<HeaderOnlyBlock, Self>::new(
                    self.chain_store.cheap_clone(),
                    self.chain_client(),
                    self.logger_factory
                        .component_logger("EthereumFirehoseBlockIngestor", None),
                    self.name.clone(),
                );
                let ingestor = ingestor.with_transforms(vec![Transforms::EthereumHeaderOnly]);

                Box::new(ingestor)
            }
            ChainClient::Rpc(rpc) => {
                let eth_adapter = rpc
                    .cheapest()
                    .ok_or_else(|| anyhow!("unable to get adapter for ethereum block ingestor"))?;
                let logger = self
                    .logger_factory
                    .component_logger(
                        "EthereumPollingBlockIngestor",
                        Some(ComponentLoggerConfig {
                            elastic: Some(ElasticComponentLoggerConfig {
                                index: String::from("block-ingestor-logs"),
                            }),
                        }),
                    )
                    .new(o!("provider" => eth_adapter.provider().to_string()));

                if !self.is_ingestible {
                    bail!(
                        "Not starting block ingestor (chain is defective), network_name {}",
                        &self.name
                    );
                }

                // The block ingestor must be configured to keep at least REORG_THRESHOLD ancestors,
                // because the json-rpc BlockStream expects blocks after the reorg threshold to be
                // present in the DB.
                Box::new(PollingBlockIngestor::new(
                    logger,
                    graph::env::ENV_VARS.reorg_threshold,
                    eth_adapter,
                    self.chain_store().cheap_clone(),
                    self.polling_ingestor_interval,
                    self.name.clone(),
                )?)
            }
        };

        Ok(ingestor)
    }
}

/// This is used in `EthereumAdapter::triggers_in_block`, called when re-processing a block for
/// newly created data sources. This allows the re-processing to be reorg safe without having to
/// always fetch the full block data.
#[derive(Clone, Debug)]
pub enum BlockFinality {
    /// If a block is final, we only need the header and the triggers.
    Final(Arc<LightEthereumBlock>),

    // If a block may still be reorged, we need to work with more local data.
    NonFinal(EthereumBlockWithCalls),
}

impl Default for BlockFinality {
    fn default() -> Self {
        Self::Final(Arc::default())
    }
}

impl BlockFinality {
    pub(crate) fn light_block(&self) -> &Arc<LightEthereumBlock> {
        match self {
            BlockFinality::Final(block) => block,
            BlockFinality::NonFinal(block) => &block.ethereum_block.block,
        }
    }
}

impl<'a> From<&'a BlockFinality> for BlockPtr {
    fn from(block: &'a BlockFinality) -> BlockPtr {
        match block {
            BlockFinality::Final(b) => BlockPtr::from(&**b),
            BlockFinality::NonFinal(b) => BlockPtr::from(&b.ethereum_block),
        }
    }
}

impl Block for BlockFinality {
    fn ptr(&self) -> BlockPtr {
        match self {
            BlockFinality::Final(block) => block.block_ptr(),
            BlockFinality::NonFinal(block) => block.ethereum_block.block.block_ptr(),
        }
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        match self {
            BlockFinality::Final(block) => block.parent_ptr(),
            BlockFinality::NonFinal(block) => block.ethereum_block.block.parent_ptr(),
        }
    }

    fn data(&self) -> Result<json::Value, json::Error> {
        // The serialization here very delicately depends on how the
        // `ChainStore`'s `blocks` and `ancestor_block` return the data we
        // store here. This should be fixed in a better way to ensure we
        // serialize/deserialize appropriately.
        //
        // Commit #d62e9846 inadvertently introduced a variation in how
        // chain stores store ethereum blocks in that they now sometimes
        // store an `EthereumBlock` that has a `block` field with a
        // `LightEthereumBlock`, and sometimes they just store the
        // `LightEthereumBlock` directly. That causes issues because the
        // code reading from the chain store always expects the JSON data to
        // have the form of an `EthereumBlock`.
        //
        // Even though this bug is fixed now and we always use the
        // serialization of an `EthereumBlock`, there are still chain stores
        // in existence that used the old serialization form, and we need to
        // deal with that when deserializing
        //
        // see also 7736e440-4c6b-11ec-8c4d-b42e99f52061
        match self {
            BlockFinality::Final(block) => {
                let eth_block = EthereumBlock {
                    block: block.clone(),
                    transaction_receipts: vec![],
                };
                json::to_value(eth_block)
            }
            BlockFinality::NonFinal(block) => json::to_value(&block.ethereum_block),
        }
    }
}

pub struct DummyDataSourceTemplate;

pub struct TriggersAdapter {
    logger: Logger,
    ethrpc_metrics: Arc<SubgraphEthRpcMetrics>,
    chain_store: Arc<dyn ChainStore>,
    chain_client: Arc<ChainClient<Chain>>,
    capabilities: NodeCapabilities,
    unified_api_version: UnifiedMappingApiVersion,
}

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter {
    async fn scan_triggers(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        filter: &TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<Chain>>, Error> {
        blocks_with_triggers(
            self.chain_client.rpc()?.cheapest_with(&self.capabilities)?,
            self.logger.clone(),
            self.chain_store.clone(),
            self.ethrpc_metrics.clone(),
            from,
            to,
            filter,
            self.unified_api_version.clone(),
        )
        .await
    }

    async fn triggers_in_block(
        &self,
        logger: &Logger,
        block: BlockFinality,
        filter: &TriggerFilter,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        let block = get_calls(
            &self.chain_client,
            logger.clone(),
            self.ethrpc_metrics.clone(),
            &self.capabilities,
            filter.requires_traces(),
            block,
        )
        .await?;

        match &block {
            BlockFinality::Final(_) => {
                let adapter = self.chain_client.rpc()?.cheapest_with(&self.capabilities)?;
                let block_number = block.number() as BlockNumber;
                let blocks = blocks_with_triggers(
                    adapter,
                    logger.clone(),
                    self.chain_store.clone(),
                    self.ethrpc_metrics.clone(),
                    block_number,
                    block_number,
                    filter,
                    self.unified_api_version.clone(),
                )
                .await?;
                assert!(blocks.len() == 1);
                Ok(blocks.into_iter().next().unwrap())
            }
            BlockFinality::NonFinal(full_block) => {
                let mut triggers = Vec::new();
                triggers.append(&mut parse_log_triggers(
                    &filter.log,
                    &full_block.ethereum_block,
                ));
                triggers.append(&mut parse_call_triggers(&filter.call, full_block)?);
                triggers.append(&mut parse_block_triggers(&filter.block, full_block));
                Ok(BlockWithTriggers::new(block, triggers, logger))
            }
        }
    }

    async fn is_on_main_chain(&self, ptr: BlockPtr) -> Result<bool, Error> {
        self.chain_client
            .rpc()?
            .cheapest()
            .ok_or(anyhow!("unable to get adapter for is_on_main_chain"))?
            .is_on_main_chain(&self.logger, ptr.clone())
            .await
    }

    async fn ancestor_block(
        &self,
        ptr: BlockPtr,
        offset: BlockNumber,
    ) -> Result<Option<BlockFinality>, Error> {
        let block: Option<EthereumBlock> = self
            .chain_store
            .cheap_clone()
            .ancestor_block(ptr, offset)
            .await?
            .map(json::from_value)
            .transpose()?;
        Ok(block.map(|block| {
            BlockFinality::NonFinal(EthereumBlockWithCalls {
                ethereum_block: block,
                calls: None,
            })
        }))
    }

    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        use futures::stream::Stream;
        use graph::prelude::LightEthereumBlockExt;

        let block = match self.chain_client.as_ref() {
            ChainClient::Firehose(_) => Some(BlockPtr {
                hash: BlockHash::from(vec![0xff; 32]),
                number: block.number.saturating_sub(1),
            }),
            ChainClient::Rpc(adapters) => {
                let blocks = adapters
                    .cheapest_with(&self.capabilities)?
                    .load_blocks(
                        self.logger.cheap_clone(),
                        self.chain_store.cheap_clone(),
                        HashSet::from_iter(Some(block.hash_as_h256())),
                    )
                    .collect()
                    .compat()
                    .await?;
                assert_eq!(blocks.len(), 1);

                blocks[0].parent_ptr()
            }
        };

        Ok(block)
    }
}

pub struct FirehoseMapper {
    adapter: Arc<dyn TriggersAdapterTrait<Chain>>,
    filter: Arc<TriggerFilter>,
}

#[async_trait]
impl BlockStreamMapper<Chain> for FirehoseMapper {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<BlockFinality>, Error> {
        let block = match output {
            Some(block) => codec::Block::decode(block)?,
            None => anyhow::bail!("ethereum mapper is expected to always have a block"),
        };

        // See comment(437a9f17-67cc-478f-80a3-804fe554b227) ethereum_block.calls is always Some even if calls
        // is empty
        let ethereum_block: EthereumBlockWithCalls = (&block).try_into()?;

        Ok(Some(BlockFinality::NonFinal(ethereum_block)))
    }

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: BlockFinality,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        self.adapter
            .triggers_in_block(logger, block, &self.filter)
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
        // is not part of the actual firehose::Response payload. As such, we need to decode the full
        // block which is useless.
        //
        // Check about adding basic information about the block in the firehose::Response or maybe
        // define a slimmed down stuct that would decode only a few fields and ignore all the rest.
        let block = codec::Block::decode(any_block.value.as_ref())?;

        use firehose::ForkStep::*;
        match step {
            StepNew => {
                // unwrap: Input cannot be None so output will be error or block.
                let block = self.decode_block(Some(any_block.value.as_ref()))?.unwrap();
                let block_with_triggers = self.block_with_triggers(logger, block).await?;

                Ok(BlockStreamEvent::ProcessBlock(
                    block_with_triggers,
                    FirehoseCursor::from(response.cursor.clone()),
                ))
            }

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
                unreachable!("irreversible step is not handled and should not be requested in the Firehose request")
            }

            StepUnset => {
                unreachable!("unknown step should not happen in the Firehose response")
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
        block: &BlockFinality,
    ) -> Result<BlockPtr, Error> {
        // Firehose for Ethereum has an hard-coded confirmations for finality sets to 200 block
        // behind the current block. The magic value 200 here comes from this hard-coded Firehose
        // value.
        let final_block_number = match block.number() {
            x if x >= 200 => x - 200,
            _ => 0,
        };

        self.block_ptr_for_number(logger, endpoint, final_block_number)
            .await
    }
}
