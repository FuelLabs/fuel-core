//! Entity types that contain the graph-node state.

use anyhow::{anyhow, bail, Error};
use hex;
use rand::rngs::OsRng;
use rand::Rng;
use std::collections::BTreeSet;
use std::str::FromStr;
use std::{fmt, fmt::Display};

use super::DeploymentHash;
use crate::blockchain::Blockchain;
use crate::data::graphql::TryFromValue;
use crate::data::store::Value;
use crate::data::subgraph::SubgraphManifest;
use crate::prelude::*;
use crate::schema::EntityType;
use crate::util::stable_hash_glue::impl_stable_hash;

pub const POI_TABLE: &str = "poi2$";

#[derive(Copy, Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubgraphHealth {
    /// Syncing without errors.
    Healthy,

    /// Syncing but has errors.
    Unhealthy,

    /// No longer syncing due to fatal error.
    Failed,
}

impl SubgraphHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubgraphHealth::Healthy => "healthy",
            SubgraphHealth::Unhealthy => "unhealthy",
            SubgraphHealth::Failed => "failed",
        }
    }

    pub fn is_failed(&self) -> bool {
        match self {
            SubgraphHealth::Failed => true,
            SubgraphHealth::Healthy | SubgraphHealth::Unhealthy => false,
        }
    }
}

impl FromStr for SubgraphHealth {
    type Err = Error;

    fn from_str(s: &str) -> Result<SubgraphHealth, Error> {
        match s {
            "healthy" => Ok(SubgraphHealth::Healthy),
            "unhealthy" => Ok(SubgraphHealth::Unhealthy),
            "failed" => Ok(SubgraphHealth::Failed),
            _ => Err(anyhow!("failed to parse `{}` as SubgraphHealth", s)),
        }
    }
}

impl From<SubgraphHealth> for String {
    fn from(health: SubgraphHealth) -> String {
        health.as_str().to_string()
    }
}

impl From<SubgraphHealth> for Value {
    fn from(health: SubgraphHealth) -> Value {
        String::from(health).into()
    }
}

impl From<SubgraphHealth> for q::Value {
    fn from(health: SubgraphHealth) -> q::Value {
        q::Value::Enum(health.into())
    }
}

impl From<SubgraphHealth> for r::Value {
    fn from(health: SubgraphHealth) -> r::Value {
        r::Value::Enum(health.into())
    }
}

impl TryFromValue for SubgraphHealth {
    fn try_from_value(value: &r::Value) -> Result<SubgraphHealth, Error> {
        match value {
            r::Value::Enum(health) => SubgraphHealth::from_str(health),
            _ => Err(anyhow!(
                "cannot parse value as SubgraphHealth: `{:?}`",
                value
            )),
        }
    }
}

/// The deployment data that is needed to create a deployment
pub struct DeploymentCreate {
    pub manifest: SubgraphManifestEntity,
    pub start_block: Option<BlockPtr>,
    pub graft_base: Option<DeploymentHash>,
    pub graft_block: Option<BlockPtr>,
    pub debug_fork: Option<DeploymentHash>,
    pub history_blocks: Option<i32>,
}

impl DeploymentCreate {
    pub fn new(
        raw_manifest: String,
        source_manifest: &SubgraphManifest<impl Blockchain>,
        start_block: Option<BlockPtr>,
    ) -> Self {
        Self {
            manifest: SubgraphManifestEntity::new(raw_manifest, source_manifest, Vec::new()),
            start_block: start_block.cheap_clone(),
            graft_base: None,
            graft_block: None,
            debug_fork: None,
            history_blocks: None,
        }
    }

    pub fn with_history_blocks(mut self, blocks: i32) -> Self {
        self.history_blocks = Some(blocks);
        self
    }

    pub fn graft(mut self, base: Option<(DeploymentHash, BlockPtr)>) -> Self {
        if let Some((subgraph, ptr)) = base {
            self.graft_base = Some(subgraph);
            self.graft_block = Some(ptr);
        }
        self
    }

    pub fn debug(mut self, fork: Option<DeploymentHash>) -> Self {
        self.debug_fork = fork;
        self
    }

    pub fn entities_with_causality_region(
        mut self,
        entities_with_causality_region: BTreeSet<EntityType>,
    ) -> Self {
        self.manifest.entities_with_causality_region =
            entities_with_causality_region.into_iter().collect();
        self
    }
}

/// The representation of a subgraph deployment when reading an existing
/// deployment
#[derive(Debug)]
pub struct SubgraphDeploymentEntity {
    pub manifest: SubgraphManifestEntity,
    pub failed: bool,
    pub health: SubgraphHealth,
    pub synced: bool,
    pub fatal_error: Option<SubgraphError>,
    pub non_fatal_errors: Vec<SubgraphError>,
    /// The earliest block for which we have data
    pub earliest_block_number: BlockNumber,
    /// The block at which indexing initially started
    pub start_block: Option<BlockPtr>,
    pub latest_block: Option<BlockPtr>,
    pub graft_base: Option<DeploymentHash>,
    pub graft_block: Option<BlockPtr>,
    pub debug_fork: Option<DeploymentHash>,
    pub reorg_count: i32,
    pub current_reorg_depth: i32,
    pub max_reorg_depth: i32,
}

#[derive(Debug)]
pub struct SubgraphManifestEntity {
    pub spec_version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub features: Vec<String>,
    pub schema: String,
    pub raw_yaml: Option<String>,
    pub entities_with_causality_region: Vec<EntityType>,
    pub history_blocks: BlockNumber,
}

impl SubgraphManifestEntity {
    pub fn new(
        raw_yaml: String,
        manifest: &super::SubgraphManifest<impl Blockchain>,
        entities_with_causality_region: Vec<EntityType>,
    ) -> Self {
        Self {
            spec_version: manifest.spec_version.to_string(),
            description: manifest.description.clone(),
            repository: manifest.repository.clone(),
            features: manifest.features.iter().map(|f| f.to_string()).collect(),
            schema: manifest.schema.document_string(),
            raw_yaml: Some(raw_yaml),
            entities_with_causality_region,
            history_blocks: BLOCK_NUMBER_MAX,
        }
    }

    pub fn template_idx_and_name(&self) -> Result<Vec<(i32, String)>, Error> {
        #[derive(Debug, Deserialize)]
        struct MinimalDs {
            name: String,
        }
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct MinimalManifest {
            data_sources: Vec<MinimalDs>,
            #[serde(default)]
            templates: Vec<MinimalDs>,
        }

        let raw_yaml = match &self.raw_yaml {
            Some(raw_yaml) => raw_yaml,
            None => bail!("raw_yaml not present"),
        };

        let manifest: MinimalManifest = serde_yaml::from_str(raw_yaml)?;

        let ds_len = manifest.data_sources.len() as i32;
        let template_idx_and_name = manifest
            .templates
            .iter()
            .map(|t| t.name.clone())
            .enumerate()
            .map(move |(idx, name)| (ds_len + idx as i32, name))
            .collect();

        Ok(template_idx_and_name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubgraphError {
    pub subgraph_id: DeploymentHash,
    pub message: String,
    pub block_ptr: Option<BlockPtr>,
    pub handler: Option<String>,

    // `true` if we are certain the error is deterministic. If in doubt, this is `false`.
    pub deterministic: bool,
}

impl Display for SubgraphError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.message)?;
        if let Some(handler) = &self.handler {
            write!(f, " in handler `{}`", handler)?;
        }
        if let Some(block_ptr) = &self.block_ptr {
            write!(f, " at block {}", block_ptr)?;
        }
        Ok(())
    }
}

impl_stable_hash!(SubgraphError {
    subgraph_id,
    message,
    block_ptr,
    handler,
    deterministic
});

pub fn generate_entity_id() -> String {
    // Fast crypto RNG from operating system
    let mut rng = OsRng::default();

    // 128 random bits
    let id_bytes: [u8; 16] = rng.gen();

    // 32 hex chars
    // Comparable to uuidv4, but without the hyphens,
    // and without spending bits on a version identifier.
    hex::encode(id_bytes)
}
