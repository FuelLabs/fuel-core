use crate::capabilities::NodeCapabilities;
use crate::Chain;
use graph::blockchain as bc;
use graph::blockchain::DataSource;
use graph::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, _data_sources: impl Iterator<Item = &'a DataSource> + Clone) {}

    fn node_capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {}
    }

    fn extend_with_template(
        &mut self,
        _data_source: impl Iterator<Item = <Chain as bc::Blockchain>::DataSourceTemplate>,
    ) {
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        vec![]
    }
}