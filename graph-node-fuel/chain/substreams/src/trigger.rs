use std::sync::Arc;

use anyhow::Error;
use graph::{
    blockchain::{
        self, block_stream::BlockWithTriggers, BlockPtr, EmptyNodeCapabilities, MappingTriggerTrait,
    },
    components::{
        store::{DeploymentLocator, SubgraphFork},
        subgraph::{MappingError, ProofOfIndexingEvent, SharedProofOfIndexing},
    },
    data_source,
    prelude::{
        anyhow, async_trait, BlockHash, BlockNumber, BlockState, CheapClone, RuntimeHostBuilder,
    },
    slog::Logger,
    substreams::Modules,
};
use graph_runtime_wasm::module::ToAscPtr;
use lazy_static::__Deref;

use crate::{Block, Chain, NoopDataSourceTemplate, ParsedChanges};

#[derive(Eq, PartialEq, PartialOrd, Ord, Debug)]
pub struct TriggerData {}

impl MappingTriggerTrait for TriggerData {
    fn error_context(&self) -> String {
        "Failed to process substreams block".to_string()
    }
}

impl blockchain::TriggerData for TriggerData {
    // TODO(filipe): Can this be improved with some data from the block?
    fn error_context(&self) -> String {
        "Failed to process substreams block".to_string()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl ToAscPtr for TriggerData {
    // substreams doesn't rely on wasm on the graph-node so this is not needed.
    fn to_asc_ptr<H: graph::runtime::AscHeap>(
        self,
        _heap: &mut H,
        _gas: &graph::runtime::gas::GasCounter,
    ) -> Result<graph::runtime::AscPtr<()>, graph::runtime::HostExportError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TriggerFilter {
    pub(crate) modules: Option<Modules>,
    pub(crate) module_name: String,
    pub(crate) start_block: Option<BlockNumber>,
    pub(crate) data_sources_len: u8,
    // the handler to call for subgraph mappings, if this is set then the binary block content
    // should be passed to the mappings.
    pub(crate) mapping_handler: Option<String>,
}

#[cfg(debug_assertions)]
impl TriggerFilter {
    pub fn modules(&self) -> &Option<Modules> {
        &self.modules
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn start_block(&self) -> &Option<BlockNumber> {
        &self.start_block
    }

    pub fn data_sources_len(&self) -> u8 {
        self.data_sources_len
    }
}

// TriggerFilter should bypass all triggers and just rely on block since all the data received
// should already have been processed.
impl blockchain::TriggerFilter<Chain> for TriggerFilter {
    fn extend_with_template(&mut self, _data_source: impl Iterator<Item = NoopDataSourceTemplate>) {
    }

    /// this function is not safe to call multiple times, only one DataSource is supported for
    ///
    fn extend<'a>(
        &mut self,
        mut data_sources: impl Iterator<Item = &'a crate::DataSource> + Clone,
    ) {
        let Self {
            modules,
            module_name,
            start_block,
            data_sources_len,
            mapping_handler,
        } = self;

        if *data_sources_len >= 1 {
            return;
        }

        if let Some(ds) = data_sources.next() {
            *data_sources_len = 1;
            *modules = ds.source.package.modules.clone();
            *module_name = ds.source.module_name.clone();
            *start_block = ds.initial_block;
            *mapping_handler = ds.mapping.handler.as_ref().map(|h| h.handler.clone());
        }
    }

    fn node_capabilities(&self) -> EmptyNodeCapabilities<Chain> {
        EmptyNodeCapabilities::default()
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        unimplemented!("this should never be called for this type")
    }
}

pub struct TriggersAdapter {}

#[async_trait]
impl blockchain::TriggersAdapter<Chain> for TriggersAdapter {
    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<Block>, Error> {
        unimplemented!()
    }

    async fn scan_triggers(
        &self,
        _from: BlockNumber,
        _to: BlockNumber,
        _filter: &TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<Chain>>, Error> {
        unimplemented!()
    }

    async fn triggers_in_block(
        &self,
        _logger: &Logger,
        _block: Block,
        _filter: &TriggerFilter,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        unimplemented!()
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        unimplemented!()
    }

    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        // This seems to work for a lot of the firehose chains.
        Ok(Some(BlockPtr {
            hash: BlockHash::from(vec![0xff; 32]),
            number: block.number.saturating_sub(1),
        }))
    }
}

fn write_poi_event(
    proof_of_indexing: &SharedProofOfIndexing,
    poi_event: &ProofOfIndexingEvent,
    causality_region: &str,
    logger: &Logger,
) {
    if let Some(proof_of_indexing) = proof_of_indexing {
        let mut proof_of_indexing = proof_of_indexing.deref().borrow_mut();
        proof_of_indexing.write(logger, causality_region, poi_event);
    }
}

pub struct TriggerProcessor {
    pub locator: DeploymentLocator,
}

impl TriggerProcessor {
    pub fn new(locator: DeploymentLocator) -> Self {
        Self { locator }
    }
}

#[async_trait]
impl<T> graph::prelude::TriggerProcessor<Chain, T> for TriggerProcessor
where
    T: RuntimeHostBuilder<Chain>,
{
    async fn process_trigger(
        &self,
        logger: &Logger,
        _: Box<dyn Iterator<Item = &T::Host> + Send + '_>,
        block: &Arc<Block>,
        _trigger: &data_source::TriggerData<Chain>,
        mut state: BlockState<Chain>,
        proof_of_indexing: &SharedProofOfIndexing,
        causality_region: &str,
        _debug_fork: &Option<Arc<dyn SubgraphFork>>,
        _subgraph_metrics: &Arc<graph::prelude::SubgraphInstanceMetrics>,
        _instrument: bool,
    ) -> Result<BlockState<Chain>, MappingError> {
        for parsed_change in block.parsed_changes.clone().into_iter() {
            match parsed_change {
                ParsedChanges::Unset => {
                    // Potentially an issue with the server side or
                    // we are running an outdated version. In either case we should abort.
                    return Err(MappingError::Unknown(anyhow!("Detected UNSET entity operation, either a server error or there's a new type of operation and we're running an outdated protobuf")));
                }
                ParsedChanges::Upsert { key, entity } => {
                    write_poi_event(
                        proof_of_indexing,
                        &ProofOfIndexingEvent::SetEntity {
                            entity_type: key.entity_type.as_str(),
                            id: &key.entity_id.to_string(),
                            data: &entity,
                        },
                        causality_region,
                        logger,
                    );

                    state.entity_cache.set(key, entity)?;
                }
                ParsedChanges::Delete(entity_key) => {
                    let entity_type = entity_key.entity_type.cheap_clone();
                    let id = entity_key.entity_id.clone();
                    state.entity_cache.remove(entity_key);

                    write_poi_event(
                        proof_of_indexing,
                        &ProofOfIndexingEvent::RemoveEntity {
                            entity_type: entity_type.as_str(),
                            id: &id.to_string(),
                        },
                        causality_region,
                        logger,
                    );
                }
            }
        }

        Ok(state)
    }
}
