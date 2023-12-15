use std::collections::HashSet;

use crate::data_source::PartialAccounts;
use crate::{data_source::DataSource, Chain};
use graph::blockchain as bc;
use graph::firehose::{BasicReceiptFilter, PrefixSuffixPair};
use graph::itertools::Itertools;
use graph::prelude::*;
use prost::Message;
use prost_types::Any;

const BASIC_RECEIPT_FILTER_TYPE_URL: &str =
    "type.googleapis.com/sf.near.transform.v1.BasicReceiptFilter";

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {
    pub(crate) block_filter: NearBlockFilter,
    pub(crate) receipt_filter: NearReceiptFilter,
}

impl TriggerFilter {
    pub fn to_module_params(&self) -> String {
        let matches = self.receipt_filter.accounts.iter().join(",");
        let partial_matches = self
            .receipt_filter
            .partial_accounts
            .iter()
            .map(|(starts_with, ends_with)| match (starts_with, ends_with) {
                (None, None) => unreachable!(),
                (None, Some(e)) => format!(",{}", e),
                (Some(s), None) => format!("{},", s),
                (Some(s), Some(e)) => format!("{},{}", s, e),
            })
            .join("\n");

        format!(
            "{},{}\n{}\n{}",
            self.receipt_filter.accounts.len(),
            self.receipt_filter.partial_accounts.len(),
            matches,
            partial_matches
        )
    }
}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource> + Clone) {
        let TriggerFilter {
            block_filter,
            receipt_filter,
        } = self;

        block_filter.extend(NearBlockFilter::from_data_sources(data_sources.clone()));
        receipt_filter.extend(NearReceiptFilter::from_data_sources(data_sources));
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
        let TriggerFilter {
            block_filter: block,
            receipt_filter: receipt,
        } = self;

        if block.trigger_every_block {
            return vec![];
        }

        if receipt.is_empty() {
            return vec![];
        }

        let filter = BasicReceiptFilter {
            accounts: receipt.accounts.into_iter().collect(),
            prefix_and_suffix_pairs: receipt
                .partial_accounts
                .iter()
                .map(|(prefix, suffix)| PrefixSuffixPair {
                    prefix: prefix.clone().unwrap_or("".to_string()),
                    suffix: suffix.clone().unwrap_or("".to_string()),
                })
                .collect(),
        };

        vec![Any {
            type_url: BASIC_RECEIPT_FILTER_TYPE_URL.into(),
            value: filter.encode_to_vec(),
        }]
    }
}

pub(crate) type Account = String;

/// NearReceiptFilter requires the account to be set, it will match every receipt where `source.account` is the recipient.
/// see docs: https://thegraph.com/docs/en/supported-networks/near/
#[derive(Clone, Debug, Default)]
pub(crate) struct NearReceiptFilter {
    pub accounts: HashSet<Account>,
    pub partial_accounts: HashSet<(Option<String>, Option<String>)>,
}

impl NearReceiptFilter {
    pub fn matches(&self, account: &String) -> bool {
        let NearReceiptFilter {
            accounts,
            partial_accounts,
        } = self;

        if accounts.contains(account) {
            return true;
        }

        partial_accounts.iter().any(|partial| match partial {
            (Some(prefix), Some(suffix)) => {
                account.starts_with(prefix) && account.ends_with(suffix)
            }
            (Some(prefix), None) => account.starts_with(prefix),
            (None, Some(suffix)) => account.ends_with(suffix),
            (None, None) => unreachable!(),
        })
    }

    pub fn is_empty(&self) -> bool {
        let NearReceiptFilter {
            accounts,
            partial_accounts,
        } = self;

        accounts.is_empty() && partial_accounts.is_empty()
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        struct Source {
            account: Option<String>,
            partial_accounts: Option<PartialAccounts>,
        }

        // Select any ds with either partial or exact accounts.
        let sources: Vec<Source> = iter
            .into_iter()
            .filter(|data_source| {
                (data_source.source.account.is_some() || data_source.source.accounts.is_some())
                    && !data_source.mapping.receipt_handlers.is_empty()
            })
            .map(|ds| Source {
                account: ds.source.account.clone(),
                partial_accounts: ds.source.accounts.clone(),
            })
            .collect();

        // Handle exact matches
        let accounts: Vec<String> = sources
            .iter()
            .filter(|s| s.account.is_some())
            .map(|s| s.account.as_ref().cloned().unwrap())
            .collect();

        // Parse all the partial accounts, produces all possible combinations of the values
        // eg:
        // prefix [a,b] and suffix [d] would produce [a,d], [b,d]
        // prefix [a] and suffix [c,d] would produce [a,c], [a,d]
        // prefix [] and suffix [c, d] would produce [None, c], [None, d]
        // prefix [a,b] and suffix [] would produce [a, None], [b, None]
        let partial_accounts: Vec<(Option<String>, Option<String>)> = sources
            .iter()
            .filter(|s| s.partial_accounts.is_some())
            .flat_map(|s| {
                let partials = s.partial_accounts.as_ref().unwrap();

                let mut pairs: Vec<(Option<String>, Option<String>)> = vec![];
                let prefixes: Vec<Option<String>> = if partials.prefixes.is_empty() {
                    vec![None]
                } else {
                    partials
                        .prefixes
                        .iter()
                        .filter(|s| !s.is_empty())
                        .map(|s| Some(s.clone()))
                        .collect()
                };

                let suffixes: Vec<Option<String>> = if partials.suffixes.is_empty() {
                    vec![None]
                } else {
                    partials
                        .suffixes
                        .iter()
                        .filter(|s| !s.is_empty())
                        .map(|s| Some(s.clone()))
                        .collect()
                };

                for prefix in prefixes.into_iter() {
                    for suffix in suffixes.iter() {
                        pairs.push((prefix.clone(), suffix.clone()))
                    }
                }

                pairs
            })
            .collect();

        Self {
            accounts: HashSet::from_iter(accounts),
            partial_accounts: HashSet::from_iter(partial_accounts),
        }
    }

    pub fn extend(&mut self, other: NearReceiptFilter) {
        let NearReceiptFilter {
            accounts,
            partial_accounts,
        } = self;

        accounts.extend(other.accounts);
        partial_accounts.extend(other.partial_accounts);
    }
}

/// NearBlockFilter will match every block regardless of source being set.
/// see docs: https://thegraph.com/docs/en/supported-networks/near/
#[derive(Clone, Debug, Default)]
pub(crate) struct NearBlockFilter {
    pub trigger_every_block: bool,
}

impl NearBlockFilter {
    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        Self {
            trigger_every_block: iter
                .into_iter()
                .any(|data_source| !data_source.mapping.block_handlers.is_empty()),
        }
    }

    pub fn extend(&mut self, other: NearBlockFilter) {
        self.trigger_every_block = self.trigger_every_block || other.trigger_every_block;
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::NearBlockFilter;
    use crate::adapter::{NearReceiptFilter, TriggerFilter, BASIC_RECEIPT_FILTER_TYPE_URL};
    use graph::{
        blockchain::TriggerFilter as _,
        firehose::{BasicReceiptFilter, PrefixSuffixPair},
    };
    use prost::Message;
    use prost_types::Any;
    use trigger_filters::NearFilter;

    #[test]
    fn near_trigger_empty_filter() {
        let filter = TriggerFilter {
            block_filter: NearBlockFilter {
                trigger_every_block: false,
            },
            receipt_filter: super::NearReceiptFilter {
                accounts: HashSet::new(),
                partial_accounts: HashSet::new(),
            },
        };
        assert_eq!(filter.to_module_params(), "0,0\n\n");
        assert_eq!(filter.to_firehose_filter(), vec![]);
    }

    #[test]
    fn near_trigger_filter_match_all_block() {
        let filter = TriggerFilter {
            block_filter: NearBlockFilter {
                trigger_every_block: true,
            },
            receipt_filter: super::NearReceiptFilter {
                accounts: HashSet::from_iter(vec!["acc1".into(), "acc2".into(), "acc3".into()]),
                partial_accounts: HashSet::new(),
            },
        };

        let filter = filter.to_firehose_filter();
        assert_eq!(filter.len(), 0);
    }

    #[test]
    fn near_trigger_filter() {
        let filter = TriggerFilter {
            block_filter: NearBlockFilter {
                trigger_every_block: false,
            },
            receipt_filter: super::NearReceiptFilter {
                accounts: HashSet::from_iter(vec!["acc1".into(), "acc2".into(), "acc3".into()]),
                partial_accounts: HashSet::new(),
            },
        };

        let filter = filter.to_firehose_filter();
        assert_eq!(filter.len(), 1);

        let firehose_filter = decode_filter(filter);

        assert_eq!(
            firehose_filter.accounts,
            vec![
                String::from("acc1"),
                String::from("acc2"),
                String::from("acc3")
            ],
        );
    }

    #[test]
    fn near_trigger_partial_filter() {
        let filter = TriggerFilter {
            block_filter: NearBlockFilter {
                trigger_every_block: false,
            },
            receipt_filter: super::NearReceiptFilter {
                accounts: HashSet::from_iter(vec!["acc1".into()]),
                partial_accounts: HashSet::from_iter(vec![
                    (Some("acc1".into()), None),
                    (None, Some("acc2".into())),
                    (Some("acc3".into()), Some("acc4".into())),
                ]),
            },
        };

        let filter = filter.to_firehose_filter();
        assert_eq!(filter.len(), 1);

        let firehose_filter = decode_filter(filter);
        assert_eq!(firehose_filter.accounts, vec![String::from("acc1"),],);

        let expected_pairs = vec![
            PrefixSuffixPair {
                prefix: "acc3".to_string(),
                suffix: "acc4".to_string(),
            },
            PrefixSuffixPair {
                prefix: "".to_string(),
                suffix: "acc2".to_string(),
            },
            PrefixSuffixPair {
                prefix: "acc1".to_string(),
                suffix: "".to_string(),
            },
        ];

        let pairs = firehose_filter.prefix_and_suffix_pairs;
        assert_eq!(pairs.len(), 3);
        assert_eq!(
            true,
            expected_pairs.iter().all(|x| pairs.contains(x)),
            "{:?}",
            pairs
        );
    }

    #[test]
    fn test_near_filter_params_serialization() -> anyhow::Result<()> {
        struct Case<'a> {
            name: &'a str,
            input: NearReceiptFilter,
            expected: NearFilter<'a>,
        }

        let cases = vec![
            Case {
                name: "empty",
                input: NearReceiptFilter::default(),
                expected: NearFilter::default(),
            },
            Case {
                name: "only full matches",
                input: super::NearReceiptFilter {
                    accounts: HashSet::from_iter(vec!["acc1".into()]),
                    partial_accounts: HashSet::new(),
                },
                expected: NearFilter {
                    accounts: HashSet::from_iter(vec!["acc1"]),
                    partial_accounts: HashSet::default(),
                },
            },
            Case {
                name: "only partial matches",
                input: super::NearReceiptFilter {
                    accounts: HashSet::new(),
                    partial_accounts: HashSet::from_iter(vec![(Some("acc1".into()), None)]),
                },
                expected: NearFilter {
                    accounts: HashSet::default(),
                    partial_accounts: HashSet::from_iter(vec![(Some("acc1"), None)]),
                },
            },
            Case {
                name: "both 1len matches",
                input: super::NearReceiptFilter {
                    accounts: HashSet::from_iter(vec!["acc1".into()]),
                    partial_accounts: HashSet::from_iter(vec![(Some("s1".into()), None)]),
                },
                expected: NearFilter {
                    accounts: HashSet::from_iter(vec!["acc1"]),
                    partial_accounts: HashSet::from_iter(vec![(Some("s1"), None)]),
                },
            },
            Case {
                name: "more partials matches",
                input: super::NearReceiptFilter {
                    accounts: HashSet::from_iter(vec!["acc1".into()]),
                    partial_accounts: HashSet::from_iter(vec![
                        (Some("s1".into()), None),
                        (None, Some("s3".into())),
                        (Some("s2".into()), Some("s2".into())),
                    ]),
                },
                expected: NearFilter {
                    accounts: HashSet::from_iter(vec!["acc1"]),
                    partial_accounts: HashSet::from_iter(vec![
                        (Some("s1"), None),
                        (None, Some("s3")),
                        (Some("s2"), Some("s2")),
                    ]),
                },
            },
            Case {
                name: "both matches",
                input: NearReceiptFilter {
                    accounts: HashSet::from_iter(vec![
                        "acc1".into(),
                        "=12-30786jhasdgmasd".into(),
                        "^&%^&^$".into(),
                        "acc3".into(),
                    ]),
                    partial_accounts: HashSet::from_iter(vec![
                        (Some("1.2.2.3.45.5".into()), None),
                        (None, Some("kjysdfoiua6sd".into())),
                        (Some("120938pokasd".into()), Some("102938poai[sd]".into())),
                    ]),
                },
                expected: NearFilter {
                    accounts: HashSet::from_iter(vec![
                        "acc1",
                        "=12-30786jhasdgmasd",
                        "^&%^&^$",
                        "acc3",
                    ]),
                    partial_accounts: HashSet::from_iter(vec![
                        (Some("1.2.2.3.45.5"), None),
                        (None, Some("kjysdfoiua6sd")),
                        (Some("120938pokasd"), Some("102938poai[sd]")),
                    ]),
                },
            },
        ];

        for case in cases.into_iter() {
            let tf = TriggerFilter {
                block_filter: NearBlockFilter::default(),
                receipt_filter: case.input,
            };
            let param = tf.to_module_params();
            let filter = NearFilter::try_from(param.as_str()).expect(&format!(
                "case: {}, the filter to parse params correctly",
                case.name
            ));

            assert_eq!(
                filter, case.expected,
                "case {},param:\n{}",
                case.name, param
            );
        }

        Ok(())
    }

    fn decode_filter(firehose_filter: Vec<Any>) -> BasicReceiptFilter {
        let firehose_filter = firehose_filter[0].clone();
        assert_eq!(
            firehose_filter.type_url,
            String::from(BASIC_RECEIPT_FILTER_TYPE_URL),
        );
        let mut bytes = &firehose_filter.value[..];
        let mut firehose_filter =
            BasicReceiptFilter::decode(&mut bytes).expect("unable to parse basic receipt filter");
        firehose_filter.accounts.sort();

        firehose_filter
    }
}
