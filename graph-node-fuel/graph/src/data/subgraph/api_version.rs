use itertools::Itertools;
use semver::Version;
use std::collections::BTreeSet;
use thiserror::Error;

pub const API_VERSION_0_0_2: Version = Version::new(0, 0, 2);

/// This version adds a new subgraph validation step that rejects manifests whose mappings have
/// different API versions if at least one of them is equal to or higher than `0.0.5`.
pub const API_VERSION_0_0_5: Version = Version::new(0, 0, 5);

// Adds two new fields to the Transaction object: nonce and input
pub const API_VERSION_0_0_6: Version = Version::new(0, 0, 6);

/// Enables event handlers to require transaction receipts in the runtime.
pub const API_VERSION_0_0_7: Version = Version::new(0, 0, 7);

/// Enables validation for fields that doesnt exist in the schema for an entity.
pub const API_VERSION_0_0_8: Version = Version::new(0, 0, 8);

/// Before this check was introduced, there were already subgraphs in the wild with spec version
/// 0.0.3, due to confusion with the api version. To avoid breaking those, we accept 0.0.3 though it
/// doesn't exist.
pub const SPEC_VERSION_0_0_3: Version = Version::new(0, 0, 3);

/// This version supports subgraph feature management.
pub const SPEC_VERSION_0_0_4: Version = Version::new(0, 0, 4);

/// This version supports event handlers having access to transaction receipts.
pub const SPEC_VERSION_0_0_5: Version = Version::new(0, 0, 5);

/// Enables the Fast POI calculation variant.
pub const SPEC_VERSION_0_0_6: Version = Version::new(0, 0, 6);

/// Enables offchain data sources.
pub const SPEC_VERSION_0_0_7: Version = Version::new(0, 0, 7);

/// Enables polling block handlers and initialisation handlers.
pub const SPEC_VERSION_0_0_8: Version = Version::new(0, 0, 8);

// Enables `endBlock` feature.
pub const SPEC_VERSION_0_0_9: Version = Version::new(0, 0, 9);

// Enables `indexerHints` feature.
pub const SPEC_VERSION_0_1_0: Version = Version::new(0, 1, 0);

pub const MIN_SPEC_VERSION: Version = Version::new(0, 0, 2);

#[derive(Clone, PartialEq, Debug)]
pub struct UnifiedMappingApiVersion(Option<Version>);

impl UnifiedMappingApiVersion {
    pub fn equal_or_greater_than(&self, other_version: &Version) -> bool {
        assert!(
            other_version >= &API_VERSION_0_0_5,
            "api versions before 0.0.5 should not be used for comparison"
        );
        match &self.0 {
            Some(version) => version >= other_version,
            None => false,
        }
    }

    pub(super) fn try_from_versions(
        versions: impl Iterator<Item = Version>,
    ) -> Result<Self, DifferentMappingApiVersions> {
        let unique_versions: BTreeSet<Version> = versions.collect();

        let all_below_referential_version = unique_versions.iter().all(|v| *v < API_VERSION_0_0_5);
        let all_the_same = unique_versions.len() == 1;

        let unified_version: Option<Version> = match (all_below_referential_version, all_the_same) {
            (false, false) => return Err(DifferentMappingApiVersions(unique_versions)),
            (false, true) => Some(unique_versions.iter().next().unwrap().clone()),
            (true, _) => None,
        };

        Ok(UnifiedMappingApiVersion(unified_version))
    }

    pub fn version(&self) -> Option<&Version> {
        self.0.as_ref()
    }
}

pub(super) fn format_versions(versions: &BTreeSet<Version>) -> String {
    versions.iter().map(ToString::to_string).join(", ")
}

#[derive(Error, Debug, PartialEq)]
#[error("Expected a single apiVersion for mappings. Found: {}.", format_versions(.0))]
pub struct DifferentMappingApiVersions(pub BTreeSet<Version>);

#[test]
fn unified_mapping_api_version_from_iterator() {
    let input = [
        vec![Version::new(0, 0, 5), Version::new(0, 0, 5)], // Ok(Some(0.0.5))
        vec![Version::new(0, 0, 6), Version::new(0, 0, 6)], // Ok(Some(0.0.6))
        vec![Version::new(0, 0, 3), Version::new(0, 0, 4)], // Ok(None)
        vec![Version::new(0, 0, 4), Version::new(0, 0, 4)], // Ok(None)
        vec![Version::new(0, 0, 3), Version::new(0, 0, 5)], // Err({0.0.3, 0.0.5})
        vec![Version::new(0, 0, 6), Version::new(0, 0, 5)], // Err({0.0.5, 0.0.6})
    ];
    let output: [Result<UnifiedMappingApiVersion, DifferentMappingApiVersions>; 6] = [
        Ok(UnifiedMappingApiVersion(Some(Version::new(0, 0, 5)))),
        Ok(UnifiedMappingApiVersion(Some(Version::new(0, 0, 6)))),
        Ok(UnifiedMappingApiVersion(None)),
        Ok(UnifiedMappingApiVersion(None)),
        Err(DifferentMappingApiVersions(
            input[4].iter().cloned().collect::<BTreeSet<Version>>(),
        )),
        Err(DifferentMappingApiVersions(
            input[5].iter().cloned().collect::<BTreeSet<Version>>(),
        )),
    ];
    for (version_vec, expected_unified_version) in input.iter().zip(output.iter()) {
        let unified = UnifiedMappingApiVersion::try_from_versions(version_vec.iter().cloned());
        match (unified, expected_unified_version) {
            (Ok(a), Ok(b)) => assert_eq!(a, *b),
            (Err(a), Err(b)) => assert_eq!(a, *b),
            _ => panic!(),
        }
    }
}
