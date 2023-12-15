use graph::{
    blockchain::{Blockchain, TriggersAdapter},
    components::{
        store::{DeploymentLocator, SubgraphFork, WritableStore},
        subgraph::ProofOfIndexingVersion,
    },
    data::subgraph::{SubgraphFeature, UnifiedMappingApiVersion},
    data_source::DataSourceTemplate,
    prelude::BlockNumber,
};
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct IndexingInputs<C: Blockchain> {
    pub deployment: DeploymentLocator,
    pub features: BTreeSet<SubgraphFeature>,
    pub start_blocks: Vec<BlockNumber>,
    pub end_blocks: BTreeSet<BlockNumber>,
    pub stop_block: Option<BlockNumber>,
    pub store: Arc<dyn WritableStore>,
    pub debug_fork: Option<Arc<dyn SubgraphFork>>,
    pub triggers_adapter: Arc<dyn TriggersAdapter<C>>,
    pub chain: Arc<C>,
    pub templates: Arc<Vec<DataSourceTemplate<C>>>,
    pub unified_api_version: UnifiedMappingApiVersion,
    pub static_filters: bool,
    pub poi_version: ProofOfIndexingVersion,
    pub network: String,

    /// Whether to instrument trigger processing and log additional,
    /// possibly expensive and noisy, information
    pub instrument: bool,
}

impl<C: Blockchain> IndexingInputs<C> {
    pub fn with_store(&self, store: Arc<dyn WritableStore>) -> Self {
        let IndexingInputs {
            deployment,
            features,
            start_blocks,
            end_blocks,
            stop_block,
            store: _,
            debug_fork,
            triggers_adapter,
            chain,
            templates,
            unified_api_version,
            static_filters,
            poi_version,
            network,
            instrument,
        } = self;
        IndexingInputs {
            deployment: deployment.clone(),
            features: features.clone(),
            start_blocks: start_blocks.clone(),
            end_blocks: end_blocks.clone(),
            stop_block: stop_block.clone(),
            store,
            debug_fork: debug_fork.clone(),
            triggers_adapter: triggers_adapter.clone(),
            chain: chain.clone(),
            templates: templates.clone(),
            unified_api_version: unified_api_version.clone(),
            static_filters: *static_filters,
            poi_version: *poi_version,
            network: network.clone(),
            instrument: *instrument,
        }
    }
}
