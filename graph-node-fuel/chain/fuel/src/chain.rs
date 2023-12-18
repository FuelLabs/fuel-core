use graph::env::EnvVars;
use graph::prelude::{MetricsRegistry, TryFutureExt};
use graph::schema::InputSchema;
use graph::substreams::{Clock, Package};
use graph::{
    anyhow::Result,
    blockchain::{
        block_stream::{
            BlockStreamEvent, BlockWithTriggers, FirehoseError,
            FirehoseMapper as FirehoseMapperTrait, TriggersAdapter as TriggersAdapterTrait,
        },
        firehose_block_stream::FirehoseBlockStream,
        BlockHash, BlockPtr, Blockchain, EmptyNodeCapabilities, IngestorError,
        RuntimeAdapter as RuntimeAdapterTrait,
    },
    components::store::DeploymentLocator,
    firehose::{self as firehose, ForkStep},
    prelude::{async_trait, o, BlockNumber, ChainStore, Error, Logger, LoggerFactory},
};
use prost::Message;
use std::sync::Arc;
use graph::blockchain::{BasicBlockchainBuilder, BlockchainBuilder};

use graph::blockchain::block_stream::{
    BlockStream, BlockStreamBuilder, BlockStreamMapper, FirehoseCursor,
};
use graph::blockchain::client::ChainClient;
use graph::blockchain::substreams_block_stream::SubstreamsBlockStream;
use graph::data::subgraph::UnifiedMappingApiVersion;
use crate::adapter::TriggerFilter;


pub struct FuelStreamBuilder {}

pub struct FirehoseMapper {
    adapter: Arc<dyn TriggersAdapterTrait<Chain>>,
    filter: Arc<TriggerFilter>,
}

pub struct TriggersAdapter {}

const FUEL_FILTER_MODULE_NAME: &str = "fuel_filter";
const SUBSTREAMS_TRIGGER_FILTER_BYTES: &[u8; 510162] = include_bytes!(
    "../../../substreams/substreams-trigger-filter/substreams-trigger-filter-v0.1.0.spkg"
);

#[async_trait]
impl BlockStreamBuilder<Chain> for FuelStreamBuilder {
    async fn build_substreams(
        &self,
        chain: &Chain,
        _schema: InputSchema,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<Chain as Blockchain>::TriggerFilter>,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        let mapper = Arc::new(FirehoseMapper {
            adapter: Arc::new(TriggersAdapter {}),
            filter,
        });
        let mut package =
            Package::decode(SUBSTREAMS_TRIGGER_FILTER_BYTES.to_vec().as_ref()).unwrap();
        match package.modules.as_mut() {
            Some(modules) => modules
                .modules
                .iter_mut()
                .find(|module| module.name == FUEL_FILTER_MODULE_NAME)
                .map(|module| {
                    graph::substreams::patch_module_params(
                        mapper.filter.to_module_params(),
                        module,
                    );
                    module
                }),
            None => None,
        };

        let logger = chain
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "SubstreamsBlockStream"));
        let start_block = subgraph_current_block
            .as_ref()
            .map(|b| b.number)
            .unwrap_or_default();

        Ok(Box::new(SubstreamsBlockStream::new(
            deployment.hash,
            chain.chain_client(),
            subgraph_current_block,
            block_cursor.as_ref().clone(),
            mapper,
            package.modules.clone(),
            FUEL_FILTER_MODULE_NAME.to_string(),
            vec![start_block],
            vec![],
            logger,
            chain.metrics_registry.clone(),
        )))
    }

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
        let adapter = chain
            .triggers_adapter(
                &deployment,
                &EmptyNodeCapabilities::default(),
                unified_api_version,
            )
            .unwrap_or_else(|_| panic!("no adapter for network {}", chain.name));

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
            chain.metrics_registry.clone(),
        )))
    }

    async fn build_polling(
        &self,
        _chain: &Chain,
        _deployment: DeploymentLocator,
        _start_blocks: Vec<BlockNumber>,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<<Chain as Blockchain>::TriggerFilter>,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        todo!()
    }
}

pub struct Chain {
    logger_factory: LoggerFactory,
    name: String,
    client: Arc<ChainClient<Self>>,
    chain_store: Arc<dyn ChainStore>,
    metrics_registry: Arc<MetricsRegistry>,
    block_stream_builder: Arc<dyn BlockStreamBuilder<Self>>,
    prefer_substreams: bool,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: near")
    }
}

impl BlockchainBuilder<Chain> for BasicBlockchainBuilder {
    fn build(self, config: &Arc<EnvVars>) -> Chain {
        Chain {
            logger_factory: self.logger_factory,
            name: self.name,
            chain_store: self.chain_store,
            client: Arc::new(ChainClient::new_firehose(self.firehose_endpoints)),
            metrics_registry: self.metrics_registry,
            block_stream_builder: Arc::new(FuelStreamBuilder {}),
            prefer_substreams: config.prefer_substreams_block_streams,
        }
    }
}