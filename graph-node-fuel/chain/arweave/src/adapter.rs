use crate::{data_source::DataSource, Chain};
use graph::blockchain as bc;
use graph::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const MATCH_ALL_WILDCARD: &str = "";
// Size of sha256(pubkey)
const SHA256_LEN: usize = 32;

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {
    pub(crate) block_filter: ArweaveBlockFilter,
    pub(crate) transaction_filter: ArweaveTransactionFilter,
}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource> + Clone) {
        let TriggerFilter {
            block_filter,
            transaction_filter,
        } = self;

        block_filter.extend(ArweaveBlockFilter::from_data_sources(data_sources.clone()));
        transaction_filter.extend(ArweaveTransactionFilter::from_data_sources(data_sources));
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
        vec![]
    }
}

/// ArweaveBlockFilter will match every block regardless of source being set.
/// see docs: https://thegraph.com/docs/en/supported-networks/arweave/
#[derive(Clone, Debug, Default)]
pub(crate) struct ArweaveTransactionFilter {
    owners_pubkey: HashSet<Vec<u8>>,
    owners_sha: HashSet<Vec<u8>>,
    match_all: bool,
}

impl ArweaveTransactionFilter {
    pub fn matches(&self, owner: &[u8]) -> bool {
        if self.match_all {
            return true;
        }

        if owner.len() == SHA256_LEN {
            return self.owners_sha.contains(owner);
        }

        self.owners_pubkey.contains(owner) || self.owners_sha.contains(&sha256(owner))
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        let owners: Vec<Vec<u8>> = iter
            .into_iter()
            .filter(|data_source| {
                data_source.source.owner.is_some()
                    && !data_source.mapping.transaction_handlers.is_empty()
            })
            .map(|ds| match &ds.source.owner {
                Some(str) if MATCH_ALL_WILDCARD.eq(str) => MATCH_ALL_WILDCARD.as_bytes().to_owned(),
                owner => base64_url::decode(&owner.clone().unwrap_or_default()).unwrap_or_default(),
            })
            .collect();

        let (owners_sha, long) = owners
            .into_iter()
            .partition::<Vec<Vec<u8>>, _>(|owner| owner.len() == SHA256_LEN);

        let (owners_pubkey, wildcard) = long
            .into_iter()
            .partition::<Vec<Vec<u8>>, _>(|long| long.len() != MATCH_ALL_WILDCARD.len());

        let match_all = !wildcard.is_empty();

        let owners_sha: Vec<Vec<u8>> = owners_sha
            .into_iter()
            .chain::<Vec<Vec<u8>>>(owners_pubkey.iter().map(|long| sha256(long)).collect())
            .collect();

        Self {
            match_all,
            owners_pubkey: HashSet::from_iter(owners_pubkey),
            owners_sha: HashSet::from_iter(owners_sha),
        }
    }

    pub fn extend(&mut self, other: ArweaveTransactionFilter) {
        let ArweaveTransactionFilter {
            owners_pubkey,
            owners_sha,
            match_all,
        } = self;

        owners_pubkey.extend(other.owners_pubkey);
        owners_sha.extend(other.owners_sha);
        *match_all = *match_all || other.match_all;
    }
}

/// ArweaveBlockFilter will match every block regardless of source being set.
/// see docs: https://thegraph.com/docs/en/supported-networks/arweave/
#[derive(Clone, Debug, Default)]
pub(crate) struct ArweaveBlockFilter {
    pub trigger_every_block: bool,
}

impl ArweaveBlockFilter {
    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        Self {
            trigger_every_block: iter
                .into_iter()
                .any(|data_source| !data_source.mapping.block_handlers.is_empty()),
        }
    }

    pub fn extend(&mut self, other: ArweaveBlockFilter) {
        self.trigger_every_block = self.trigger_every_block || other.trigger_every_block;
    }
}

fn sha256(bs: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(bs);
    let res = hasher.finalize();
    res.to_vec()
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use graph::{prelude::Link, semver::Version};

    use crate::data_source::{DataSource, Mapping, Source, TransactionHandler};

    use super::{ArweaveTransactionFilter, MATCH_ALL_WILDCARD};

    const ARWEAVE_PUBKEY_EXAMPLE: &str = "x-62w7g2yKACOgP_d04bhG8IX-AWgPrxHl2JgZBDdNLfAsidiiAaoIZPeM8K5gGvl7-8QVk79YV4OC878Ey0gXi7Atj5BouRyXnFMjJcPVXVyBoYCBuG7rJDDmh4_Ilon6vVOuHVIZ47Vb0tcgsxgxdvVFC2mn9N_SBl23pbeICNJZYOH57kf36gicuV_IwYSdqlQ0HQ_psjmg8EFqO7xzvAMP5HKW3rqTrYZxbCew2FkM734ysWckT39TpDBPx3HrFOl6obUdQWkHNOeKyzcsKFDywNgVWZOb89CYU7JFYlwX20io39ZZv0UJUOEFNjtVHkT_s0_A2O9PltsrZLLlQXZUuYASdbAPD2g_qXfhmPBZ0SXPWCDY-UVwVN1ncwYmk1F_i35IA8kAKsajaltD2wWDQn9g5mgJAWWn2xhLqkbwGbdwQMRD0-0eeuy1uzCooJQCC_bPJksoqkYwB9SGOjkayf4r4oZ2QDY4FicCsswz4Od_gud30ZWyHjWgqGzSFYFzawDBS1Gr_nu_q5otFrv20ZGTxYqGsLHWq4VHs6KjsQvzgBjfyb0etqHQEPJJmbQmY3LSogR4bxdReUHhj2EK9xIB-RKzDvDdL7fT5K0V9MjbnC2uktA0VjLlvwJ64_RhbQhxdp_zR39r-zyCXT-brPEYW1-V7Ey9K3XUE";
    const ARWEAVE_SHA_EXAMPLE: &str = "ahLxjCMCHr1ZE72VDDoaK4IKiLUUpeuo8t-M6y23DXw";

    #[test]
    fn transaction_filter_wildcard_matches_all() {
        let dss = vec![
            new_datasource(None, 10),
            new_datasource(Some(base64_url::encode(MATCH_ALL_WILDCARD)), 10),
            new_datasource(Some(base64_url::encode("owner")), 10),
            new_datasource(Some(ARWEAVE_PUBKEY_EXAMPLE.into()), 10),
        ];

        let dss: Vec<&DataSource> = dss.iter().collect();

        let filter = ArweaveTransactionFilter::from_data_sources(dss);
        assert_eq!(true, filter.matches("asdas".as_bytes()))
    }

    #[test]
    fn transaction_filter_match() {
        let dss = vec![
            new_datasource(None, 10),
            new_datasource(Some(ARWEAVE_PUBKEY_EXAMPLE.into()), 10),
        ];

        let dss: Vec<&DataSource> = dss.iter().collect();

        let filter = ArweaveTransactionFilter::from_data_sources(dss);
        assert_eq!(false, filter.matches("asdas".as_bytes()));
        assert_eq!(
            true,
            filter.matches(
                &base64_url::decode(ARWEAVE_SHA_EXAMPLE).expect("failed to parse sha example")
            )
        );
        assert_eq!(
            true,
            filter.matches(
                &base64_url::decode(ARWEAVE_PUBKEY_EXAMPLE).expect("failed to parse PK example")
            )
        )
    }

    #[test]
    fn transaction_filter_extend_match() {
        let dss = vec![
            new_datasource(None, 10),
            new_datasource(Some(ARWEAVE_SHA_EXAMPLE.into()), 10),
        ];

        let dss: Vec<&DataSource> = dss.iter().collect();

        let filter = ArweaveTransactionFilter::from_data_sources(dss);
        assert_eq!(false, filter.matches("asdas".as_bytes()));
        assert_eq!(
            true,
            filter.matches(
                &base64_url::decode(ARWEAVE_SHA_EXAMPLE).expect("failed to parse sha example")
            )
        );
        assert_eq!(
            true,
            filter.matches(
                &base64_url::decode(ARWEAVE_PUBKEY_EXAMPLE).expect("failed to parse PK example")
            )
        )
    }

    #[test]
    fn transaction_filter_extend_wildcard_matches_all() {
        let dss = vec![
            new_datasource(None, 10),
            new_datasource(Some(MATCH_ALL_WILDCARD.into()), 10),
            new_datasource(Some("owner".into()), 10),
        ];

        let dss: Vec<&DataSource> = dss.iter().collect();

        let mut filter = ArweaveTransactionFilter::default();

        filter.extend(ArweaveTransactionFilter::from_data_sources(dss));
        assert_eq!(true, filter.matches("asdas".as_bytes()));
        assert_eq!(true, filter.matches(ARWEAVE_PUBKEY_EXAMPLE.as_bytes()));
        assert_eq!(true, filter.matches(ARWEAVE_SHA_EXAMPLE.as_bytes()))
    }

    fn new_datasource(owner: Option<String>, start_block: i32) -> DataSource {
        DataSource {
            kind: "".into(),
            network: None,
            name: "".into(),
            source: Source {
                owner,
                start_block,
                end_block: None,
            },
            mapping: Mapping {
                api_version: Version::new(1, 2, 3),
                language: "".into(),
                entities: vec![],
                block_handlers: vec![],
                transaction_handlers: vec![TransactionHandler {
                    handler: "my_handler".into(),
                }],
                runtime: Arc::new(vec![]),
                link: Link { link: "".into() },
            },
            context: Arc::new(None),
            creation_block: None,
        }
    }
}
