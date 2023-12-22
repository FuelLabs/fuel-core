use crate::{
    data_source::DataSource,
    Chain,
};
use graph::{
    blockchain as bc,
    prelude::*,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct FuelBlockFilter {
    pub trigger_every_block: bool,
}

impl FuelBlockFilter {
    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        Self {
            trigger_every_block: iter
                .into_iter()
                .any(|data_source| !data_source.mapping.block_handlers.is_empty()),
        }
    }

    pub fn extend(&mut self, other: FuelBlockFilter) {
        self.trigger_every_block = self.trigger_every_block || other.trigger_every_block;
    }
}

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {
    pub(crate) block_filter: FuelBlockFilter,
}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource> + Clone) {
        let TriggerFilter { block_filter } = self;

        block_filter.extend(FuelBlockFilter::from_data_sources(data_sources.clone()));
    }

    fn node_capabilities(&self) -> bc::EmptyNodeCapabilities<Chain> {
        bc::EmptyNodeCapabilities::default()
    }

    fn extend_with_template(
        &mut self,
        _data_source: impl Iterator<Item = <Chain as bc::Blockchain>::DataSourceTemplate>,
    ) {
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        todo!()
    }
}
