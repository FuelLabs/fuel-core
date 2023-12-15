use super::Blockchain;
use crate::{
    components::store::ChainStore, env::EnvVars, firehose::FirehoseEndpoints,
    prelude::LoggerFactory, prelude::MetricsRegistry,
};
use std::sync::Arc;

/// An implementor of [`BlockchainBuilder`] for chains that don't require
/// particularly fancy builder logic.
pub struct BasicBlockchainBuilder {
    pub logger_factory: LoggerFactory,
    pub name: String,
    pub chain_store: Arc<dyn ChainStore>,
    pub firehose_endpoints: FirehoseEndpoints,
    pub metrics_registry: Arc<MetricsRegistry>,
}

/// Something that can build a [`Blockchain`].
pub trait BlockchainBuilder<C>
where
    C: Blockchain,
{
    fn build(self, config: &Arc<EnvVars>) -> C;
}
