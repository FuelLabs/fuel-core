use crate::prelude::FeatureFlag;
use itertools::Itertools;
use lazy_static::lazy_static;
use semver::{Version, VersionReq};
use std::collections::HashMap;

lazy_static! {
    static ref VERSION_COLLECTION: HashMap<Version, Vec<FeatureFlag>> = {
        vec![
            // baseline version
            (Version::new(1, 0, 0), vec![]),
        ].into_iter().collect()
    };

    // Sorted vector of versions. From higher to lower.
    pub static ref VERSIONS: Vec<&'static Version> = {
        let mut versions = VERSION_COLLECTION.keys().collect_vec().clone();
        versions.sort_by(|a, b| b.partial_cmp(a).unwrap());
        versions
    };
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    pub version: Version,
    features: Vec<FeatureFlag>,
}

impl ApiVersion {
    pub fn new(version_requirement: &VersionReq) -> Result<Self, String> {
        let version = Self::resolve(version_requirement)?;

        Ok(Self {
            version: version.clone(),
            features: VERSION_COLLECTION
                .get(version)
                .unwrap_or_else(|| panic!("Version {:?} is not supported", version))
                .clone(),
        })
    }

    pub fn from_version(version: &Version) -> Result<ApiVersion, String> {
        ApiVersion::new(
            &VersionReq::parse(version.to_string().as_str())
                .map_err(|error| format!("Invalid version requirement: {}", error))?,
        )
    }

    pub fn supports(&self, feature: FeatureFlag) -> bool {
        self.features.contains(&feature)
    }

    fn resolve(version_requirement: &VersionReq) -> Result<&Version, String> {
        for version in VERSIONS.iter() {
            if version_requirement.matches(version) {
                return Ok(version);
            }
        }

        Err("Could not resolve the version".to_string())
    }
}

impl Default for ApiVersion {
    fn default() -> Self {
        // Default to the latest version.
        // The `VersionReq::default()` returns `*` which means "any version".
        // The first matching version is the latest version.
        ApiVersion::new(&VersionReq::default()).unwrap()
    }
}
