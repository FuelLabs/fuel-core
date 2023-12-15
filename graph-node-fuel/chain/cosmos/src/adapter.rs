use std::collections::HashSet;

use prost::Message;
use prost_types::Any;

use crate::{data_source::DataSource, Chain};
use graph::blockchain as bc;
use graph::firehose::EventTypeFilter;
use graph::prelude::*;

const EVENT_TYPE_FILTER_TYPE_URL: &str =
    "type.googleapis.com/sf.cosmos.transform.v1.EventTypeFilter";

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {
    pub(crate) event_type_filter: CosmosEventTypeFilter,
    pub(crate) block_filter: CosmosBlockFilter,
}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource> + Clone) {
        self.event_type_filter
            .extend_from_data_sources(data_sources.clone());
        self.block_filter.extend_from_data_sources(data_sources);
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
        if self.block_filter.trigger_every_block {
            return vec![];
        }

        if self.event_type_filter.event_types.is_empty() {
            return vec![];
        }

        let filter = EventTypeFilter {
            event_types: Vec::from_iter(self.event_type_filter.event_types),
        };

        vec![Any {
            type_url: EVENT_TYPE_FILTER_TYPE_URL.to_string(),
            value: filter.encode_to_vec(),
        }]
    }
}

pub type EventType = String;

#[derive(Clone, Debug, Default)]
pub(crate) struct CosmosEventTypeFilter {
    pub event_types: HashSet<EventType>,
}

impl CosmosEventTypeFilter {
    pub(crate) fn matches(&self, event_type: &EventType) -> bool {
        self.event_types.contains(event_type)
    }

    fn extend_from_data_sources<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource>) {
        self.event_types.extend(
            data_sources.flat_map(|data_source| data_source.events().map(ToString::to_string)),
        );
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CosmosBlockFilter {
    pub trigger_every_block: bool,
}

impl CosmosBlockFilter {
    fn extend_from_data_sources<'a>(
        &mut self,
        mut data_sources: impl Iterator<Item = &'a DataSource>,
    ) {
        if !self.trigger_every_block {
            self.trigger_every_block = data_sources.any(DataSource::has_block_handler);
        }
    }
}

#[cfg(test)]
mod test {
    use graph::blockchain::TriggerFilter as _;

    use super::*;

    #[test]
    fn test_trigger_filters() {
        let cases = [
            (TriggerFilter::test_new(false, &[]), None),
            (TriggerFilter::test_new(true, &[]), None),
            (TriggerFilter::test_new(true, &["event_1", "event_2"]), None),
            (
                TriggerFilter::test_new(false, &["event_1", "event_2", "event_3"]),
                Some(event_type_filter_with(&["event_1", "event_2", "event_3"])),
            ),
        ];

        for (trigger_filter, expected_filter) in cases {
            let firehose_filter = trigger_filter.to_firehose_filter();
            let decoded_filter = decode_filter(firehose_filter);

            assert_eq!(decoded_filter.is_some(), expected_filter.is_some());

            if let (Some(mut expected_filter), Some(mut decoded_filter)) =
                (expected_filter, decoded_filter)
            {
                // event types may be in different order
                expected_filter.event_types.sort();
                decoded_filter.event_types.sort();

                assert_eq!(decoded_filter, expected_filter);
            }
        }
    }

    impl TriggerFilter {
        pub(crate) fn test_new(trigger_every_block: bool, event_types: &[&str]) -> TriggerFilter {
            TriggerFilter {
                event_type_filter: CosmosEventTypeFilter {
                    event_types: event_types.iter().map(ToString::to_string).collect(),
                },
                block_filter: CosmosBlockFilter {
                    trigger_every_block,
                },
            }
        }
    }

    fn event_type_filter_with(event_types: &[&str]) -> EventTypeFilter {
        EventTypeFilter {
            event_types: event_types.iter().map(ToString::to_string).collect(),
        }
    }

    fn decode_filter(proto_filters: Vec<Any>) -> Option<EventTypeFilter> {
        assert!(proto_filters.len() <= 1);

        let proto_filter = proto_filters.get(0)?;

        assert_eq!(proto_filter.type_url, EVENT_TYPE_FILTER_TYPE_URL);

        let firehose_filter = EventTypeFilter::decode(&*proto_filter.value)
            .expect("Could not decode EventTypeFilter from protobuf Any");

        Some(firehose_filter)
    }
}
