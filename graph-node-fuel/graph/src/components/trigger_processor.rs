use std::sync::Arc;

use async_trait::async_trait;
use slog::Logger;

use crate::{blockchain::Blockchain, data_source::TriggerData, prelude::SubgraphInstanceMetrics};

use super::{
    store::SubgraphFork,
    subgraph::{BlockState, MappingError, RuntimeHostBuilder, SharedProofOfIndexing},
};

#[async_trait]
pub trait TriggerProcessor<C, T>: Sync + Send
where
    C: Blockchain,
    T: RuntimeHostBuilder<C>,
{
    async fn process_trigger<'a>(
        &'a self,
        logger: &Logger,
        hosts: Box<dyn Iterator<Item = &T::Host> + Send + 'a>,
        block: &Arc<C::Block>,
        trigger: &TriggerData<C>,
        mut state: BlockState<C>,
        proof_of_indexing: &SharedProofOfIndexing,
        causality_region: &str,
        debug_fork: &Option<Arc<dyn SubgraphFork>>,
        subgraph_metrics: &Arc<SubgraphInstanceMetrics>,
        instrument: bool,
    ) -> Result<BlockState<C>, MappingError>;
}
