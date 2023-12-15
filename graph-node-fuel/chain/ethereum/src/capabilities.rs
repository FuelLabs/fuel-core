use graph::impl_slog_value;
use std::cmp::Ordering;
use std::fmt;

use crate::DataSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeCapabilities {
    pub archive: bool,
    pub traces: bool,
}

/// Two [`NodeCapabilities`] can only be compared if one is the subset of the
/// other. No [`Ord`] (i.e. total order) implementation is applicable.
impl PartialOrd for NodeCapabilities {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        product_order(&[
            self.archive.cmp(&other.archive),
            self.traces.cmp(&other.traces),
        ])
    }
}

/// Defines a [product order](https://en.wikipedia.org/wiki/Product_order) over
/// an array of [`Ordering`].
fn product_order(cmps: &[Ordering]) -> Option<Ordering> {
    if cmps.iter().all(|c| c.is_eq()) {
        Some(Ordering::Equal)
    } else if cmps.iter().all(|c| c.is_le()) {
        Some(Ordering::Less)
    } else if cmps.iter().all(|c| c.is_ge()) {
        Some(Ordering::Greater)
    } else {
        None
    }
}

impl fmt::Display for NodeCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let NodeCapabilities { archive, traces } = self;

        let mut capabilities = vec![];
        if *archive {
            capabilities.push("archive");
        }
        if *traces {
            capabilities.push("traces");
        }

        f.write_str(&capabilities.join(", "))
    }
}

impl_slog_value!(NodeCapabilities, "{}");

impl graph::blockchain::NodeCapabilities<crate::Chain> for NodeCapabilities {
    fn from_data_sources(data_sources: &[DataSource]) -> Self {
        NodeCapabilities {
            archive: data_sources.iter().any(|ds| {
                ds.mapping
                    .requires_archive()
                    .expect("failed to parse mappings")
            }),
            traces: data_sources.iter().any(|ds| {
                ds.mapping.has_call_handler() || ds.mapping.has_block_handler_with_call_filter()
            }),
        }
    }
}
