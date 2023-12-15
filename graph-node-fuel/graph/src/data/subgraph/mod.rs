/// Rust representation of the GraphQL schema for a `SubgraphManifest`.
pub mod schema;

/// API version and spec version.
pub mod api_version;
pub use api_version::*;

pub mod features;
pub mod status;

pub use features::{SubgraphFeature, SubgraphFeatureValidationError};

use crate::object;
use anyhow::{anyhow, Context, Error};
use futures03::{future::try_join, stream::FuturesOrdered, TryStreamExt as _};
use itertools::Itertools;
use semver::Version;
use serde::{de, ser};
use serde_yaml;
use slog::Logger;
use stable_hash::{FieldAddress, StableHash};
use stable_hash_legacy::SequenceNumber;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    marker::PhantomData,
};
use thiserror::Error;
use wasmparser;
use web3::types::Address;

use crate::{
    bail,
    blockchain::{BlockPtr, Blockchain, DataSource as _},
    components::{
        link_resolver::LinkResolver,
        store::{StoreError, SubgraphStore},
    },
    data::{
        graphql::TryFromValue, query::QueryExecutionError,
        subgraph::features::validate_subgraph_features,
    },
    data_source::{
        offchain::OFFCHAIN_KINDS, DataSource, DataSourceTemplate, UnresolvedDataSource,
        UnresolvedDataSourceTemplate,
    },
    ensure,
    prelude::{r, CheapClone, Value, ENV_VARS},
    schema::{InputSchema, SchemaValidationError},
};

use crate::prelude::{impl_slog_value, BlockNumber, Deserialize, Serialize};

use std::fmt;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use super::{graphql::IntoValue, value::Word};

/// Deserialize an Address (with or without '0x' prefix).
fn deserialize_address<'de, D>(deserializer: D) -> Result<Option<Address>, D::Error>
where
    D: de::Deserializer<'de>,
{
    use serde::de::Error;

    let s: String = de::Deserialize::deserialize(deserializer)?;
    let address = s.trim_start_matches("0x");
    Address::from_str(address)
        .map_err(D::Error::custom)
        .map(Some)
}

/// The IPFS hash used to identifiy a deployment externally, i.e., the
/// `Qm..` string that `graph-cli` prints when deploying to a subgraph
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct DeploymentHash(String);

impl stable_hash_legacy::StableHash for DeploymentHash {
    #[inline]
    fn stable_hash<H: stable_hash_legacy::StableHasher>(
        &self,
        mut sequence_number: H::Seq,
        state: &mut H,
    ) {
        let Self(inner) = self;
        stable_hash_legacy::StableHash::stable_hash(inner, sequence_number.next_child(), state);
    }
}

impl StableHash for DeploymentHash {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        let Self(inner) = self;
        stable_hash::StableHash::stable_hash(inner, field_address.child(0), state);
    }
}

impl_slog_value!(DeploymentHash);

/// `DeploymentHash` is fixed-length so cheap to clone.
impl CheapClone for DeploymentHash {}

impl DeploymentHash {
    /// Check that `s` is a valid `SubgraphDeploymentId` and create a new one.
    /// If `s` is longer than 46 characters, or contains characters other than
    /// alphanumeric characters or `_`, return s (as a `String`) as the error
    pub fn new(s: impl Into<String>) -> Result<Self, String> {
        let s = s.into();

        // Enforce length limit
        if s.len() > 46 {
            return Err(s);
        }

        // Check that the ID contains only allowed characters.
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(s);
        }

        // Allow only deployment id's for 'real' subgraphs, not the old
        // metadata subgraph.
        if s == "subgraphs" {
            return Err(s);
        }

        Ok(DeploymentHash(s))
    }

    pub fn to_ipfs_link(&self) -> Link {
        Link {
            link: format!("/ipfs/{}", self),
        }
    }
}

impl Deref for DeploymentHash {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for DeploymentHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ser::Serialize for DeploymentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> de::Deserialize<'de> for DeploymentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        DeploymentHash::new(s)
            .map_err(|s| de::Error::invalid_value(de::Unexpected::Str(&s), &"valid subgraph name"))
    }
}

impl TryFromValue for DeploymentHash {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        Self::new(String::try_from_value(value)?)
            .map_err(|s| anyhow!("Invalid subgraph ID `{}`", s))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubgraphName(String);

impl SubgraphName {
    pub fn new(s: impl Into<String>) -> Result<Self, ()> {
        let s = s.into();

        // Note: these validation rules must be kept consistent with the validation rules
        // implemented in any other components that rely on subgraph names.

        // Enforce length limits
        if s.is_empty() || s.len() > 255 {
            return Err(());
        }

        // Check that the name contains only allowed characters.
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/')
        {
            return Err(());
        }

        // Parse into components and validate each
        for part in s.split('/') {
            // Each part must be non-empty
            if part.is_empty() {
                return Err(());
            }

            // To keep URLs unambiguous, reserve the token "graphql"
            if part == "graphql" {
                return Err(());
            }

            // Part should not start or end with a special character.
            let first_char = part.chars().next().unwrap();
            let last_char = part.chars().last().unwrap();
            if !first_char.is_ascii_alphanumeric()
                || !last_char.is_ascii_alphanumeric()
                || !part.chars().any(|c| c.is_ascii_alphabetic())
            {
                return Err(());
            }
        }

        Ok(SubgraphName(s))
    }

    /// Tests are allowed to create arbitrary subgraph names
    #[cfg(debug_assertions)]
    pub fn new_unchecked(s: impl Into<String>) -> Self {
        SubgraphName(s.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SubgraphName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ser::Serialize for SubgraphName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> de::Deserialize<'de> for SubgraphName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        SubgraphName::new(s.clone())
            .map_err(|()| de::Error::invalid_value(de::Unexpected::Str(&s), &"valid subgraph name"))
    }
}

/// Result of a creating a subgraph in the registar.
#[derive(Serialize)]
pub struct CreateSubgraphResult {
    /// The ID of the subgraph that was created.
    pub id: String,
}

#[derive(Error, Debug)]
pub enum SubgraphRegistrarError {
    #[error("subgraph resolve error: {0}")]
    ResolveError(SubgraphManifestResolveError),
    #[error("subgraph already exists: {0}")]
    NameExists(String),
    #[error("subgraph name not found: {0}")]
    NameNotFound(String),
    #[error("network not supported by registrar: {0}")]
    NetworkNotSupported(Error),
    #[error("deployment not found: {0}")]
    DeploymentNotFound(String),
    #[error("deployment assignment unchanged: {0}")]
    DeploymentAssignmentUnchanged(String),
    #[error("subgraph registrar internal query error: {0}")]
    QueryExecutionError(#[from] QueryExecutionError),
    #[error("subgraph registrar error with store: {0}")]
    StoreError(StoreError),
    #[error("subgraph validation error: {}", display_vector(.0))]
    ManifestValidationError(Vec<SubgraphManifestValidationError>),
    #[error("subgraph deployment error: {0}")]
    SubgraphDeploymentError(StoreError),
    #[error("subgraph registrar error: {0}")]
    Unknown(#[from] anyhow::Error),
}

impl From<StoreError> for SubgraphRegistrarError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::DeploymentNotFound(id) => SubgraphRegistrarError::DeploymentNotFound(id),
            e => SubgraphRegistrarError::StoreError(e),
        }
    }
}

impl From<SubgraphManifestValidationError> for SubgraphRegistrarError {
    fn from(e: SubgraphManifestValidationError) -> Self {
        SubgraphRegistrarError::ManifestValidationError(vec![e])
    }
}

#[derive(Error, Debug)]
pub enum SubgraphAssignmentProviderError {
    #[error("Subgraph resolve error: {0}")]
    ResolveError(Error),
    /// Occurs when attempting to remove a subgraph that's not hosted.
    #[error("Subgraph with ID {0} already running")]
    AlreadyRunning(DeploymentHash),
    #[error("Subgraph provider error: {0}")]
    Unknown(#[from] anyhow::Error),
}

impl From<::diesel::result::Error> for SubgraphAssignmentProviderError {
    fn from(e: ::diesel::result::Error) -> Self {
        SubgraphAssignmentProviderError::Unknown(e.into())
    }
}

#[derive(Error, Debug)]
pub enum SubgraphManifestValidationError {
    #[error("subgraph has no data sources")]
    NoDataSources,
    #[error("subgraph source address is required")]
    SourceAddressRequired,
    #[error("subgraph cannot index data from different Ethereum networks")]
    MultipleEthereumNetworks,
    #[error("subgraph must have at least one Ethereum network data source")]
    EthereumNetworkRequired,
    #[error("the specified block must exist on the Ethereum network")]
    BlockNotFound(String),
    #[error("schema validation failed: {0:?}")]
    SchemaValidationError(Vec<SchemaValidationError>),
    #[error("the graft base is invalid: {0}")]
    GraftBaseInvalid(String),
    #[error("subgraph must use a single apiVersion across its data sources. Found: {}", format_versions(&(.0).0))]
    DifferentApiVersions(#[from] DifferentMappingApiVersions),
    #[error(transparent)]
    FeatureValidationError(#[from] SubgraphFeatureValidationError),
    #[error("data source {0} is invalid: {1}")]
    DataSourceValidation(String, Error),
}

#[derive(Error, Debug)]
pub enum SubgraphManifestResolveError {
    #[error("parse error: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("subgraph is not UTF-8")]
    NonUtf8,
    #[error("subgraph is not valid YAML")]
    InvalidFormat,
    #[error("resolve error: {0}")]
    ResolveError(#[from] anyhow::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceContext(HashMap<Word, Value>);

impl DataSourceContext {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    // This collects the entries into an ordered vector so that it can be iterated deterministically.
    pub fn sorted(self) -> Vec<(Word, Value)> {
        let mut v: Vec<_> = self.0.into_iter().collect();
        v.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        v
    }
}

impl From<HashMap<Word, Value>> for DataSourceContext {
    fn from(map: HashMap<Word, Value>) -> Self {
        Self(map)
    }
}

/// IPLD link.
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct Link {
    #[serde(rename = "/")]
    pub link: String,
}

impl<S: ToString> From<S> for Link {
    fn from(s: S) -> Self {
        Self {
            link: s.to_string(),
        }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct UnresolvedSchema {
    pub file: Link,
}

impl UnresolvedSchema {
    pub async fn resolve(
        self,
        id: DeploymentHash,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
    ) -> Result<InputSchema, anyhow::Error> {
        let schema_bytes = resolver
            .cat(logger, &self.file)
            .await
            .with_context(|| format!("failed to resolve schema {}", &self.file.link))?;
        InputSchema::parse(&String::from_utf8(schema_bytes)?, id)
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The contract address for the data source. We allow data sources
    /// without an address for 'wildcard' triggers that catch all possible
    /// events with the given `abi`
    #[serde(default, deserialize_with = "deserialize_address")]
    pub address: Option<Address>,
    pub abi: String,
    #[serde(default)]
    pub start_block: BlockNumber,
    pub end_block: Option<BlockNumber>,
}

pub fn calls_host_fn(runtime: &[u8], host_fn: &str) -> anyhow::Result<bool> {
    use wasmparser::Payload;

    for payload in wasmparser::Parser::new(0).parse_all(runtime) {
        if let Payload::ImportSection(s) = payload? {
            for import in s {
                let import = import?;
                if import.field == Some(host_fn) {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graft {
    pub base: DeploymentHash,
    pub block: BlockNumber,
}

impl Graft {
    async fn validate<S: SubgraphStore>(
        &self,
        store: Arc<S>,
    ) -> Result<(), SubgraphManifestValidationError> {
        use SubgraphManifestValidationError::*;

        let last_processed_block = store
            .least_block_ptr(&self.base)
            .await
            .map_err(|e| GraftBaseInvalid(e.to_string()))?;
        let is_base_healthy = store
            .is_healthy(&self.base)
            .await
            .map_err(|e| GraftBaseInvalid(e.to_string()))?;

        // We are being defensive here: we don't know which specific
        // instance of a subgraph we will use as the base for the graft,
        // since the notion of which of these instances is active can change
        // between this check and when the graft actually happens when the
        // subgraph is started. We therefore check that any instance of the
        // base subgraph is suitable.
        match (last_processed_block, is_base_healthy) {
            (None, _) => Err(GraftBaseInvalid(format!(
                "failed to graft onto `{}` since it has not processed any blocks",
                self.base
            ))),
            (Some(ptr), true) if ptr.number < self.block => Err(GraftBaseInvalid(format!(
                "failed to graft onto `{}` at block {} since it has only processed block {}",
                self.base, self.block, ptr.number
            ))),
            // If the base deployment is failed *and* the `graft.block` is not
            // less than the `base.block`, the graft shouldn't be permitted.
            //
            // The developer should change their `graft.block` in the manifest
            // to `base.block - 1` or less.
            (Some(ptr), false) if self.block >= ptr.number => Err(GraftBaseInvalid(format!(
                "failed to graft onto `{}` at block {} since it's not healthy. You can graft it starting at block {} backwards",
                self.base, self.block, ptr.number - 1
            ))),
            (Some(_), _) => Ok(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DeploymentFeatures {
    pub id: String,
    pub spec_version: String,
    pub api_version: Option<String>,
    pub features: Vec<String>,
    pub data_source_kinds: Vec<String>,
    pub network: String,
    pub handler_kinds: Vec<String>,
}

impl IntoValue for DeploymentFeatures {
    fn into_value(self) -> r::Value {
        object! {
            __typename: "SubgraphFeatures",
            specVersion: self.spec_version,
            apiVersion: self.api_version,
            features: self.features,
            dataSources: self.data_source_kinds,
            handlers: self.handler_kinds,
            network: self.network,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseSubgraphManifest<C, S, D, T> {
    pub id: DeploymentHash,
    pub spec_version: Version,
    #[serde(default)]
    pub features: BTreeSet<SubgraphFeature>,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub schema: S,
    pub data_sources: Vec<D>,
    pub graft: Option<Graft>,
    #[serde(default)]
    pub templates: Vec<T>,
    #[serde(skip_serializing, default)]
    pub chain: PhantomData<C>,
    pub indexer_hints: Option<IndexerHints>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerHints {
    pub history_blocks: Option<BlockNumber>,
}

/// SubgraphManifest with IPFS links unresolved
pub type UnresolvedSubgraphManifest<C> = BaseSubgraphManifest<
    C,
    UnresolvedSchema,
    UnresolvedDataSource<C>,
    UnresolvedDataSourceTemplate<C>,
>;

/// SubgraphManifest validated with IPFS links resolved
pub type SubgraphManifest<C> =
    BaseSubgraphManifest<C, InputSchema, DataSource<C>, DataSourceTemplate<C>>;

/// Unvalidated SubgraphManifest
pub struct UnvalidatedSubgraphManifest<C: Blockchain>(SubgraphManifest<C>);

impl<C: Blockchain> UnvalidatedSubgraphManifest<C> {
    /// Entry point for resolving a subgraph definition.
    /// Right now the only supported links are of the form:
    /// `/ipfs/QmUmg7BZC1YP1ca66rRtWKxpXp77WgVHrnv263JtDuvs2k`
    pub async fn resolve(
        id: DeploymentHash,
        raw: serde_yaml::Mapping,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        max_spec_version: semver::Version,
    ) -> Result<Self, SubgraphManifestResolveError> {
        Ok(Self(
            SubgraphManifest::resolve_from_raw(id, raw, resolver, logger, max_spec_version).await?,
        ))
    }

    /// Validates the subgraph manifest file.
    ///
    /// Graft base validation will be skipped if the parameter `validate_graft_base` is false.
    pub async fn validate<S: SubgraphStore>(
        self,
        store: Arc<S>,
        validate_graft_base: bool,
    ) -> Result<SubgraphManifest<C>, Vec<SubgraphManifestValidationError>> {
        let mut errors: Vec<SubgraphManifestValidationError> = vec![];

        // Validate that the manifest has at least one data source
        if self.0.data_sources.is_empty() {
            errors.push(SubgraphManifestValidationError::NoDataSources);
        }

        for ds in &self.0.data_sources {
            errors.extend(ds.validate().into_iter().map(|e| {
                SubgraphManifestValidationError::DataSourceValidation(ds.name().to_owned(), e)
            }));
        }

        // For API versions newer than 0.0.5, validate that all mappings uses the same api_version
        if let Err(different_api_versions) = self.0.unified_mapping_api_version() {
            errors.push(different_api_versions.into());
        };

        let mut networks = self
            .0
            .data_sources
            .iter()
            .filter_map(|d| Some(d.as_onchain()?.network()?.to_string()))
            .collect::<Vec<String>>();
        networks.sort();
        networks.dedup();
        match networks.len() {
            0 => errors.push(SubgraphManifestValidationError::EthereumNetworkRequired),
            1 => (),
            _ => errors.push(SubgraphManifestValidationError::MultipleEthereumNetworks),
        }

        if let Some(graft) = &self.0.graft {
            if validate_graft_base {
                if let Err(graft_err) = graft.validate(store).await {
                    errors.push(graft_err);
                }
            }
        }

        // Validate subgraph feature usage and declaration.
        if self.0.spec_version >= SPEC_VERSION_0_0_4 {
            if let Err(feature_validation_error) = validate_subgraph_features(&self.0) {
                errors.push(feature_validation_error.into())
            }
        }

        match errors.is_empty() {
            true => Ok(self.0),
            false => Err(errors),
        }
    }

    pub fn spec_version(&self) -> &Version {
        &self.0.spec_version
    }
}

impl<C: Blockchain> SubgraphManifest<C> {
    /// Entry point for resolving a subgraph definition.
    pub async fn resolve_from_raw(
        id: DeploymentHash,
        raw: serde_yaml::Mapping,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        max_spec_version: semver::Version,
    ) -> Result<Self, SubgraphManifestResolveError> {
        let unresolved = UnresolvedSubgraphManifest::parse(id, raw)?;

        let resolved = unresolved
            .resolve(resolver, logger, max_spec_version)
            .await?;

        Ok(resolved)
    }

    pub fn network_name(&self) -> String {
        // Assume the manifest has been validated, ensuring network names are homogenous
        self.data_sources
            .iter()
            .find_map(|d| Some(d.as_onchain()?.network()?.to_string()))
            .expect("Validated manifest does not have a network defined on any datasource")
    }

    pub fn start_blocks(&self) -> Vec<BlockNumber> {
        self.data_sources
            .iter()
            .filter_map(|d| Some(d.as_onchain()?.start_block()))
            .collect()
    }

    pub fn history_blocks(&self) -> Option<BlockNumber> {
        self.indexer_hints
            .as_ref()
            .and_then(|hints| hints.history_blocks)
    }

    pub fn api_versions(&self) -> impl Iterator<Item = semver::Version> + '_ {
        self.templates
            .iter()
            .map(|template| template.api_version())
            .chain(self.data_sources.iter().map(|source| source.api_version()))
    }

    pub fn deployment_features(&self) -> DeploymentFeatures {
        let unified_api_version = self.unified_mapping_api_version().ok();
        let network = self.network_name();
        let api_version = unified_api_version
            .map(|v| v.version().map(|v| v.to_string()))
            .flatten();

        let handler_kinds = self
            .data_sources
            .iter()
            .map(|ds| ds.handler_kinds())
            .flatten()
            .collect::<HashSet<_>>();

        let features: Vec<String> = self
            .features
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>();

        let spec_version = self.spec_version.to_string();

        let mut data_source_kinds = self
            .data_sources
            .iter()
            .map(|ds| ds.kind().to_string())
            .collect::<HashSet<_>>();

        let data_source_template_kinds = self
            .templates
            .iter()
            .map(|t| t.kind().to_string())
            .collect::<Vec<_>>();

        data_source_kinds.extend(data_source_template_kinds);
        DeploymentFeatures {
            id: self.id.to_string(),
            api_version,
            features,
            spec_version,
            data_source_kinds: data_source_kinds.into_iter().collect_vec(),
            handler_kinds: handler_kinds
                .into_iter()
                .map(|s| s.to_string())
                .collect_vec(),
            network,
        }
    }

    pub fn runtimes(&self) -> impl Iterator<Item = Arc<Vec<u8>>> + '_ {
        self.templates
            .iter()
            .filter_map(|template| template.runtime())
            .chain(
                self.data_sources
                    .iter()
                    .filter_map(|source| source.runtime()),
            )
    }

    pub fn unified_mapping_api_version(
        &self,
    ) -> Result<UnifiedMappingApiVersion, DifferentMappingApiVersions> {
        UnifiedMappingApiVersion::try_from_versions(self.api_versions())
    }

    pub fn template_idx_and_name(&self) -> impl Iterator<Item = (u32, String)> + '_ {
        // We cannot include static data sources in the map because a static data source and a
        // template may have the same name in the manifest. Duplicated with
        // `UnresolvedSubgraphManifest::template_idx_and_name`.
        let ds_len = self.data_sources.len() as u32;
        self.templates
            .iter()
            .map(|t| t.name().to_owned())
            .enumerate()
            .map(move |(idx, name)| (ds_len + idx as u32, name))
    }
}

impl<C: Blockchain> UnresolvedSubgraphManifest<C> {
    pub fn parse(
        id: DeploymentHash,
        mut raw: serde_yaml::Mapping,
    ) -> Result<Self, SubgraphManifestResolveError> {
        // Inject the IPFS hash as the ID of the subgraph into the definition.
        raw.insert("id".into(), id.to_string().into());

        serde_yaml::from_value(raw.into()).map_err(Into::into)
    }

    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        max_spec_version: semver::Version,
    ) -> Result<SubgraphManifest<C>, SubgraphManifestResolveError> {
        let UnresolvedSubgraphManifest {
            id,
            spec_version,
            features,
            description,
            repository,
            schema,
            data_sources,
            graft,
            templates,
            chain,
            indexer_hints,
        } = self;

        if !(MIN_SPEC_VERSION..=max_spec_version.clone()).contains(&spec_version) {
            return Err(anyhow!(
                "This Graph Node only supports manifest spec versions between {} and {}, but subgraph `{}` uses `{}`",
                MIN_SPEC_VERSION,
                max_spec_version,
                id,
                spec_version
            ).into());
        }

        let ds_count = data_sources.len();
        if ds_count as u64 + templates.len() as u64 > u32::MAX as u64 {
            return Err(
                anyhow!("Subgraph has too many declared data sources and templates",).into(),
            );
        }

        let schema = schema.resolve(id.clone(), resolver, logger).await?;

        let (data_sources, templates) = try_join(
            data_sources
                .into_iter()
                .enumerate()
                .map(|(idx, ds)| ds.resolve(resolver, logger, idx as u32))
                .collect::<FuturesOrdered<_>>()
                .try_collect::<Vec<_>>(),
            templates
                .into_iter()
                .enumerate()
                .map(|(idx, template)| {
                    template.resolve(resolver, &schema, logger, ds_count as u32 + idx as u32)
                })
                .collect::<FuturesOrdered<_>>()
                .try_collect::<Vec<_>>(),
        )
        .await?;

        for ds in &data_sources {
            ensure!(
                semver::VersionReq::parse(&format!("<= {}", ENV_VARS.mappings.max_api_version))
                    .unwrap()
                    .matches(&ds.api_version()),
                "The maximum supported mapping API version of this indexer is {}, but `{}` was found",
                ENV_VARS.mappings.max_api_version,
                ds.api_version()
            );
        }

        if spec_version < SPEC_VERSION_0_0_7
            && data_sources
                .iter()
                .any(|ds| OFFCHAIN_KINDS.contains_key(ds.kind().as_str()))
        {
            bail!(
                "Offchain data sources not supported prior to {}",
                SPEC_VERSION_0_0_7
            );
        }

        if spec_version < SPEC_VERSION_0_0_9
            && data_sources.iter().any(|ds| ds.end_block().is_some())
        {
            bail!(
                "Defining `endBlock` in the manifest is not supported prior to {}",
                SPEC_VERSION_0_0_9
            );
        }

        if spec_version < SPEC_VERSION_0_1_0 && indexer_hints.is_some() {
            bail!(
                "`indexerHints` are not supported prior to {}",
                SPEC_VERSION_0_1_0
            );
        }

        // Check the min_spec_version of each data source against the spec version of the subgraph
        let min_spec_version_mismatch = data_sources
            .iter()
            .find(|ds| spec_version < ds.min_spec_version());

        if let Some(min_spec_version_mismatch) = min_spec_version_mismatch {
            bail!(
                "Subgraph `{}` uses spec version {}, but data source `{}` requires at least version {}",
                id,
                spec_version,
                min_spec_version_mismatch.name(),
                min_spec_version_mismatch.min_spec_version()
            );
        }

        Ok(SubgraphManifest {
            id,
            spec_version,
            features,
            description,
            repository,
            schema,
            data_sources,
            graft,
            templates,
            chain,
            indexer_hints,
        })
    }
}

/// Important details about the current state of a subgraph deployment
/// used while executing queries against a deployment
///
/// The `reorg_count` and `max_reorg_depth` fields are maintained (in the
/// database) by `store::metadata::forward_block_ptr` and
/// `store::metadata::revert_block_ptr` which get called as part of transacting
/// new entities into the store or reverting blocks.
#[derive(Debug, Clone)]
pub struct DeploymentState {
    pub id: DeploymentHash,
    /// The number of blocks that were ever reverted in this subgraph. This
    /// number increases monotonically every time a block is reverted
    pub reorg_count: u32,
    /// The maximum number of blocks we ever reorged without moving a block
    /// forward in between
    pub max_reorg_depth: u32,
    /// The last block that the subgraph has processed
    pub latest_block: BlockPtr,
    /// The earliest block that the subgraph has processed
    pub earliest_block_number: BlockNumber,
}

impl DeploymentState {
    /// Is this subgraph deployed and has it processed any blocks?
    pub fn is_deployed(&self) -> bool {
        self.latest_block.number > 0
    }

    pub fn block_queryable(&self, block: BlockNumber) -> Result<(), String> {
        if block > self.latest_block.number {
            return Err(format!(
                "subgraph {} has only indexed up to block number {} \
                        and data for block number {} is therefore not yet available",
                self.id, self.latest_block.number, block
            ));
        }
        if block < self.earliest_block_number {
            return Err(format!(
                "subgraph {} only has data starting at block number {} \
                            and data for block number {} is therefore not available",
                self.id, self.earliest_block_number, block
            ));
        }
        Ok(())
    }
}

fn display_vector(input: &[impl std::fmt::Display]) -> impl std::fmt::Display {
    let formatted_errors = input
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
        .join("; ");
    format!("[{}]", formatted_errors)
}

#[test]
fn test_subgraph_name_validation() {
    assert!(SubgraphName::new("a").is_ok());
    assert!(SubgraphName::new("a/a").is_ok());
    assert!(SubgraphName::new("a-lOng-name_with_0ne-component").is_ok());
    assert!(SubgraphName::new("a-long-name_with_one-3omponent").is_ok());
    assert!(SubgraphName::new("a/b_c").is_ok());
    assert!(SubgraphName::new("A/Z-Z").is_ok());
    assert!(SubgraphName::new("a1/A-A").is_ok());
    assert!(SubgraphName::new("aaa/a1").is_ok());
    assert!(SubgraphName::new("1a/aaaa").is_ok());
    assert!(SubgraphName::new("aaaa/1a").is_ok());
    assert!(SubgraphName::new("2nena4test/lala").is_ok());

    assert!(SubgraphName::new("").is_err());
    assert!(SubgraphName::new("/a").is_err());
    assert!(SubgraphName::new("a/").is_err());
    assert!(SubgraphName::new("a//a").is_err());
    assert!(SubgraphName::new("a/0").is_err());
    assert!(SubgraphName::new("a/_").is_err());
    assert!(SubgraphName::new("a/a_").is_err());
    assert!(SubgraphName::new("a/_a").is_err());
    assert!(SubgraphName::new("aaaa aaaaa").is_err());
    assert!(SubgraphName::new("aaaa!aaaaa").is_err());
    assert!(SubgraphName::new("aaaa+aaaaa").is_err());
    assert!(SubgraphName::new("a/graphql").is_err());
    assert!(SubgraphName::new("graphql/a").is_err());
    assert!(SubgraphName::new("this-component-is-very-long-but-we-dont-care").is_ok());
}

#[test]
fn test_display_vector() {
    let manifest_validation_error = SubgraphRegistrarError::ManifestValidationError(vec![
        SubgraphManifestValidationError::NoDataSources,
        SubgraphManifestValidationError::SourceAddressRequired,
    ]);

    let expected_display_message =
	"subgraph validation error: [subgraph has no data sources; subgraph source address is required]"
	.to_string();

    assert_eq!(
        expected_display_message,
        format!("{}", manifest_validation_error)
    )
}
