use crate::{
    components::{
        link_resolver::LinkResolver,
        store::{BlockNumber, DeploymentCursorTracker, DeploymentLocator},
    },
    data::subgraph::UnifiedMappingApiVersion,
    prelude::DataSourceTemplateInfo,
};
use anyhow::Error;
use async_trait::async_trait;
use serde::Deserialize;
use std::{collections::HashSet, convert::TryFrom, sync::Arc};

use super::{
    block_stream::{self, BlockStream, FirehoseCursor},
    client::ChainClient,
    BlockIngestor, EmptyNodeCapabilities, HostFn, IngestorError, MappingTriggerTrait,
    TriggerWithHandler,
};

use super::{
    block_stream::BlockWithTriggers, Block, BlockPtr, Blockchain, BlockchainKind, DataSource,
    DataSourceTemplate, RuntimeAdapter, TriggerData, TriggerFilter, TriggersAdapter,
    UnresolvedDataSource, UnresolvedDataSourceTemplate,
};

#[derive(Debug)]
pub struct MockBlockchain;

#[derive(Clone, Hash, Eq, PartialEq, Debug, Default)]
pub struct MockBlock {
    pub number: u64,
}

impl Block for MockBlock {
    fn ptr(&self) -> BlockPtr {
        todo!()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        todo!()
    }
}

#[derive(Clone)]
pub struct MockDataSource {
    pub api_version: semver::Version,
    pub kind: String,
    pub network: Option<String>,
}

impl<C: Blockchain> TryFrom<DataSourceTemplateInfo<C>> for MockDataSource {
    type Error = Error;

    fn try_from(_value: DataSourceTemplateInfo<C>) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl<C: Blockchain> DataSource<C> for MockDataSource {
    fn from_template_info(_template_info: DataSourceTemplateInfo<C>) -> Result<Self, Error> {
        todo!()
    }

    fn address(&self) -> Option<&[u8]> {
        todo!()
    }

    fn start_block(&self) -> crate::components::store::BlockNumber {
        todo!()
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        vec!["mock_handler_1", "mock_handler_2"]
            .into_iter()
            .collect()
    }

    fn end_block(&self) -> Option<BlockNumber> {
        todo!()
    }

    fn name(&self) -> &str {
        todo!()
    }

    fn kind(&self) -> &str {
        self.kind.as_str()
    }

    fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    fn context(&self) -> std::sync::Arc<Option<crate::prelude::DataSourceContext>> {
        todo!()
    }

    fn creation_block(&self) -> Option<crate::components::store::BlockNumber> {
        todo!()
    }

    fn api_version(&self) -> semver::Version {
        self.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        todo!()
    }

    fn match_and_decode(
        &self,
        _trigger: &C::TriggerData,
        _block: &std::sync::Arc<C::Block>,
        _logger: &slog::Logger,
    ) -> Result<Option<TriggerWithHandler<C>>, anyhow::Error> {
        todo!()
    }

    fn is_duplicate_of(&self, _other: &Self) -> bool {
        todo!()
    }

    fn as_stored_dynamic_data_source(&self) -> crate::components::store::StoredDynamicDataSource {
        todo!()
    }

    fn from_stored_dynamic_data_source(
        _template: &C::DataSourceTemplate,
        _stored: crate::components::store::StoredDynamicDataSource,
    ) -> Result<Self, anyhow::Error> {
        todo!()
    }

    fn validate(&self) -> Vec<anyhow::Error> {
        todo!()
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct MockUnresolvedDataSource;

#[async_trait]
impl<C: Blockchain> UnresolvedDataSource<C> for MockUnresolvedDataSource {
    async fn resolve(
        self,
        _resolver: &Arc<dyn LinkResolver>,
        _logger: &slog::Logger,
        _manifest_idx: u32,
    ) -> Result<C::DataSource, anyhow::Error> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct MockDataSourceTemplate;

impl<C: Blockchain> DataSourceTemplate<C> for MockDataSourceTemplate {
    fn api_version(&self) -> semver::Version {
        todo!()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        todo!()
    }

    fn name(&self) -> &str {
        todo!()
    }

    fn manifest_idx(&self) -> u32 {
        todo!()
    }

    fn kind(&self) -> &str {
        todo!()
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct MockUnresolvedDataSourceTemplate;

#[async_trait]
impl<C: Blockchain> UnresolvedDataSourceTemplate<C> for MockUnresolvedDataSourceTemplate {
    async fn resolve(
        self,
        _resolver: &Arc<dyn LinkResolver>,
        _logger: &slog::Logger,
        _manifest_idx: u32,
    ) -> Result<C::DataSourceTemplate, anyhow::Error> {
        todo!()
    }
}

pub struct MockTriggersAdapter;

#[async_trait]
impl<C: Blockchain> TriggersAdapter<C> for MockTriggersAdapter {
    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<C::Block>, Error> {
        todo!()
    }

    async fn scan_triggers(
        &self,
        _from: crate::components::store::BlockNumber,
        _to: crate::components::store::BlockNumber,
        _filter: &C::TriggerFilter,
    ) -> Result<Vec<block_stream::BlockWithTriggers<C>>, Error> {
        todo!()
    }

    async fn triggers_in_block(
        &self,
        _logger: &slog::Logger,
        _block: C::Block,
        _filter: &C::TriggerFilter,
    ) -> Result<BlockWithTriggers<C>, Error> {
        todo!()
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        todo!()
    }

    async fn parent_ptr(&self, _block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        todo!()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MockTriggerData;

impl TriggerData for MockTriggerData {
    fn error_context(&self) -> String {
        todo!()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

#[derive(Debug)]
pub struct MockMappingTrigger {}

impl MappingTriggerTrait for MockMappingTrigger {
    fn error_context(&self) -> String {
        todo!()
    }
}
#[derive(Clone, Default)]
pub struct MockTriggerFilter;

impl<C: Blockchain> TriggerFilter<C> for MockTriggerFilter {
    fn extend<'a>(&mut self, _data_sources: impl Iterator<Item = &'a C::DataSource> + Clone) {
        todo!()
    }

    fn node_capabilities(&self) -> C::NodeCapabilities {
        todo!()
    }

    fn extend_with_template(
        &mut self,
        _data_source: impl Iterator<Item = <C as Blockchain>::DataSourceTemplate>,
    ) {
        todo!()
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        todo!()
    }
}

pub struct MockRuntimeAdapter;

impl<C: Blockchain> RuntimeAdapter<C> for MockRuntimeAdapter {
    fn host_fns(&self, _ds: &C::DataSource) -> Result<Vec<HostFn>, Error> {
        todo!()
    }
}

#[async_trait]
impl Blockchain for MockBlockchain {
    const KIND: BlockchainKind = BlockchainKind::Ethereum;

    type Client = ();
    type Block = MockBlock;

    type DataSource = MockDataSource;

    type UnresolvedDataSource = MockUnresolvedDataSource;

    type DataSourceTemplate = MockDataSourceTemplate;

    type UnresolvedDataSourceTemplate = MockUnresolvedDataSourceTemplate;

    type TriggerData = MockTriggerData;

    type MappingTrigger = MockMappingTrigger;

    type TriggerFilter = MockTriggerFilter;

    type NodeCapabilities = EmptyNodeCapabilities<Self>;

    fn triggers_adapter(
        &self,
        _loc: &crate::components::store::DeploymentLocator,
        _capabilities: &Self::NodeCapabilities,
        _unified_api_version: crate::data::subgraph::UnifiedMappingApiVersion,
    ) -> Result<std::sync::Arc<dyn TriggersAdapter<Self>>, anyhow::Error> {
        todo!()
    }

    async fn new_block_stream(
        &self,
        _deployment: DeploymentLocator,
        _store: impl DeploymentCursorTracker,
        _start_blocks: Vec<BlockNumber>,
        _filter: Arc<Self::TriggerFilter>,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Self>>, Error> {
        todo!()
    }

    fn is_refetch_block_required(&self) -> bool {
        false
    }

    async fn refetch_firehose_block(
        &self,
        _logger: &slog::Logger,
        _cursor: FirehoseCursor,
    ) -> Result<MockBlock, Error> {
        todo!()
    }

    fn chain_store(&self) -> std::sync::Arc<dyn crate::components::store::ChainStore> {
        todo!()
    }

    async fn block_pointer_from_number(
        &self,
        _logger: &slog::Logger,
        _number: crate::components::store::BlockNumber,
    ) -> Result<BlockPtr, IngestorError> {
        todo!()
    }

    fn runtime_adapter(&self) -> std::sync::Arc<dyn RuntimeAdapter<Self>> {
        todo!()
    }

    fn chain_client(&self) -> Arc<ChainClient<MockBlockchain>> {
        todo!()
    }

    fn block_ingestor(&self) -> anyhow::Result<Box<dyn BlockIngestor>> {
        todo!()
    }
}
