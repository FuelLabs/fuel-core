//! Functions to detect subgraph features.
//!
//! The rationale of this module revolves around the concept of feature declaration and detection.
//!
//! Features are declared in the `subgraph.yml` file, also known as the subgraph's manifest, and are
//! validated by a graph-node instance during the deploy phase or by direct request.
//!
//! A feature validation error will be triggered if a subgraph use any feature without declaring it
//! in the `features` section of the manifest file.
//!
//! Feature validation is performed by the [`validate_subgraph_features`] function.

use crate::{
    blockchain::Blockchain,
    data::subgraph::SubgraphManifest,
    prelude::{Deserialize, Serialize},
    schema::InputSchema,
};
use itertools::Itertools;
use std::{collections::BTreeSet, fmt, str::FromStr};

use super::calls_host_fn;

/// This array must contain all IPFS-related functions that are exported by the host WASM runtime.
///
/// For reference, search this codebase for: ff652476-e6ad-40e4-85b8-e815d6c6e5e2
const IPFS_ON_ETHEREUM_CONTRACTS_FUNCTION_NAMES: [&str; 3] =
    ["ipfs.cat", "ipfs.getBlock", "ipfs.map"];

#[derive(Debug, Deserialize, Serialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum SubgraphFeature {
    NonFatalErrors,
    Grafting,
    FullTextSearch,
    #[serde(alias = "nonDeterministicIpfs")]
    IpfsOnEthereumContracts,
}

impl fmt::Display for SubgraphFeature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        serde_plain::to_string(self)
            .map_err(|_| fmt::Error)
            .and_then(|x| write!(f, "{}", x))
    }
}

impl FromStr for SubgraphFeature {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        serde_plain::from_str(value)
            .map_err(|_error| anyhow::anyhow!("Invalid subgraph feature: {}", value))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize, thiserror::Error, Debug)]
pub enum SubgraphFeatureValidationError {
    /// A feature is used by the subgraph but it is not declared in the `features` section of the manifest file.
    #[error("The feature `{}` is used by the subgraph but it is not declared in the manifest.", fmt_subgraph_features(.0))]
    Undeclared(BTreeSet<SubgraphFeature>),

    /// The provided compiled mapping is not a valid WASM module.
    #[error("Failed to parse the provided mapping WASM module")]
    InvalidMapping,
}

fn fmt_subgraph_features(subgraph_features: &BTreeSet<SubgraphFeature>) -> String {
    subgraph_features.iter().join(", ")
}

pub fn validate_subgraph_features<C: Blockchain>(
    manifest: &SubgraphManifest<C>,
) -> Result<BTreeSet<SubgraphFeature>, SubgraphFeatureValidationError> {
    let declared: &BTreeSet<SubgraphFeature> = &manifest.features;
    let used = detect_features(manifest)?;
    let undeclared: BTreeSet<SubgraphFeature> = used.difference(declared).cloned().collect();
    if !undeclared.is_empty() {
        Err(SubgraphFeatureValidationError::Undeclared(undeclared))
    } else {
        Ok(used)
    }
}

pub fn detect_features<C: Blockchain>(
    manifest: &SubgraphManifest<C>,
) -> Result<BTreeSet<SubgraphFeature>, InvalidMapping> {
    let features = vec![
        detect_non_fatal_errors(manifest),
        detect_grafting(manifest),
        detect_full_text_search(&manifest.schema),
        detect_ipfs_on_ethereum_contracts(manifest)?,
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(features)
}

fn detect_non_fatal_errors<C: Blockchain>(
    manifest: &SubgraphManifest<C>,
) -> Option<SubgraphFeature> {
    if manifest.features.contains(&SubgraphFeature::NonFatalErrors) {
        Some(SubgraphFeature::NonFatalErrors)
    } else {
        None
    }
}

fn detect_grafting<C: Blockchain>(manifest: &SubgraphManifest<C>) -> Option<SubgraphFeature> {
    manifest.graft.as_ref().map(|_| SubgraphFeature::Grafting)
}

fn detect_full_text_search(schema: &InputSchema) -> Option<SubgraphFeature> {
    match schema.get_fulltext_directives() {
        Ok(directives) => (!directives.is_empty()).then_some(SubgraphFeature::FullTextSearch),

        Err(_) => {
            // Currently we return an error from `get_fulltext_directives` function if the
            // fullTextSearch directive is found.
            Some(SubgraphFeature::FullTextSearch)
        }
    }
}

pub struct InvalidMapping;

impl From<InvalidMapping> for SubgraphFeatureValidationError {
    fn from(_: InvalidMapping) -> Self {
        SubgraphFeatureValidationError::InvalidMapping
    }
}

fn detect_ipfs_on_ethereum_contracts<C: Blockchain>(
    manifest: &SubgraphManifest<C>,
) -> Result<Option<SubgraphFeature>, InvalidMapping> {
    for runtime in manifest.runtimes() {
        for function_name in IPFS_ON_ETHEREUM_CONTRACTS_FUNCTION_NAMES {
            if calls_host_fn(&runtime, function_name).map_err(|_| InvalidMapping)? {
                return Ok(Some(SubgraphFeature::IpfsOnEthereumContracts));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use SubgraphFeature::*;
    const VARIANTS: [SubgraphFeature; 4] = [
        NonFatalErrors,
        Grafting,
        FullTextSearch,
        IpfsOnEthereumContracts,
    ];
    const STRING: [&str; 4] = [
        "nonFatalErrors",
        "grafting",
        "fullTextSearch",
        "ipfsOnEthereumContracts",
    ];

    #[test]
    fn subgraph_feature_display() {
        for (variant, string) in VARIANTS.iter().zip(STRING.iter()) {
            assert_eq!(variant.to_string(), *string)
        }
    }

    #[test]
    fn subgraph_feature_from_str() {
        for (variant, string) in VARIANTS.iter().zip(STRING.iter()) {
            assert_eq!(SubgraphFeature::from_str(string).unwrap(), *variant)
        }
    }
}
