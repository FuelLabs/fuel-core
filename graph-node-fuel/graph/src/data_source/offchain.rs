use crate::{
    bail,
    blockchain::{BlockPtr, Blockchain},
    components::{
        link_resolver::LinkResolver,
        store::{BlockNumber, StoredDynamicDataSource},
        subgraph::DataSourceTemplateInfo,
    },
    data::{store::scalar::Bytes, subgraph::SPEC_VERSION_0_0_7, value::Word},
    data_source,
    ipfs_client::CidFile,
    prelude::{DataSourceContext, Link},
    schema::{EntityType, InputSchema},
};
use anyhow::{anyhow, Context, Error};
use itertools::Itertools;
use lazy_static::lazy_static;
use serde::Deserialize;
use slog::{info, warn, Logger};
use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{atomic::AtomicI32, Arc},
};

use super::{CausalityRegion, DataSourceCreationError, TriggerWithHandler};

lazy_static! {
    pub static ref OFFCHAIN_KINDS: HashMap<&'static str, OffchainDataSourceKind> = [
        ("file/ipfs", OffchainDataSourceKind::Ipfs),
        ("file/arweave", OffchainDataSourceKind::Arweave),
    ]
    .into_iter()
    .collect();
}

const OFFCHAIN_HANDLER_KIND: &str = "offchain";
const NOT_DONE_VALUE: i32 = -1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffchainDataSourceKind {
    Ipfs,
    Arweave,
}
impl OffchainDataSourceKind {
    pub fn try_parse_source(&self, bs: Bytes) -> Result<Source, anyhow::Error> {
        let source = match self {
            OffchainDataSourceKind::Ipfs => {
                let cid_file = CidFile::try_from(bs)?;
                Source::Ipfs(cid_file)
            }
            OffchainDataSourceKind::Arweave => {
                let base64 = Word::from(String::from_utf8(bs.to_vec())?);
                Source::Arweave(base64)
            }
        };
        Ok(source)
    }
}

impl ToString for OffchainDataSourceKind {
    fn to_string(&self) -> String {
        // This is less performant than hardcoding the values but makes it more difficult
        // to be used incorrectly, since this map is quite small it should be fine.
        OFFCHAIN_KINDS
            .iter()
            .find_map(|(str, kind)| {
                if kind.eq(self) {
                    Some(str.to_string())
                } else {
                    None
                }
            })
            // the kind is validated based on OFFCHAIN_KINDS so it's guaranteed to exist
            .unwrap()
    }
}

impl FromStr for OffchainDataSourceKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        OFFCHAIN_KINDS
            .iter()
            .find_map(|(str, kind)| if str.eq(&s) { Some(kind.clone()) } else { None })
            .ok_or(anyhow!(
                "unsupported offchain datasource kind: {s}, expected one of: {}",
                OFFCHAIN_KINDS.iter().map(|x| x.0).join(",")
            ))
    }
}

#[derive(Debug, Clone)]
pub struct DataSource {
    pub kind: OffchainDataSourceKind,
    pub name: String,
    pub manifest_idx: u32,
    pub source: Source,
    pub mapping: Mapping,
    pub context: Arc<Option<DataSourceContext>>,
    pub creation_block: Option<BlockNumber>,
    done_at: Arc<AtomicI32>,
    pub causality_region: CausalityRegion,
}

impl DataSource {
    pub fn new(
        kind: OffchainDataSourceKind,
        name: String,
        manifest_idx: u32,
        source: Source,
        mapping: Mapping,
        context: Arc<Option<DataSourceContext>>,
        creation_block: Option<BlockNumber>,
        causality_region: CausalityRegion,
    ) -> Self {
        Self {
            kind,
            name,
            manifest_idx,
            source,
            mapping,
            context,
            creation_block,
            done_at: Arc::new(AtomicI32::new(NOT_DONE_VALUE)),
            causality_region,
        }
    }

    // mark this data source as processed.
    pub fn mark_processed_at(&self, block_no: i32) {
        assert!(block_no != NOT_DONE_VALUE);
        self.done_at
            .store(block_no, std::sync::atomic::Ordering::SeqCst);
    }

    // returns `true` if the data source is processed.
    pub fn is_processed(&self) -> bool {
        self.done_at.load(std::sync::atomic::Ordering::SeqCst) != NOT_DONE_VALUE
    }

    pub fn done_at(&self) -> Option<i32> {
        match self.done_at.load(std::sync::atomic::Ordering::SeqCst) {
            NOT_DONE_VALUE => None,
            n => Some(n),
        }
    }

    pub fn set_done_at(&self, block: Option<i32>) {
        let value = block.unwrap_or(NOT_DONE_VALUE);

        self.done_at
            .store(value, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn min_spec_version(&self) -> semver::Version {
        // off-chain data sources are only supported in spec version 0.0.7 and up
        // As more and more kinds of off-chain data sources are added, this
        // function should be updated to return the minimum spec version
        // required for each kind
        SPEC_VERSION_0_0_7
    }

    pub fn handler_kind(&self) -> &str {
        OFFCHAIN_HANDLER_KIND
    }
}

impl DataSource {
    pub fn from_template_info(
        info: DataSourceTemplateInfo<impl Blockchain>,
        causality_region: CausalityRegion,
    ) -> Result<Self, DataSourceCreationError> {
        let template = match info.template {
            data_source::DataSourceTemplate::Offchain(template) => template,
            data_source::DataSourceTemplate::Onchain(_) => {
                bail!("Cannot create offchain data source from onchain template")
            }
        };
        let source = info.params.into_iter().next().ok_or(anyhow::anyhow!(
            "Failed to create data source from template `{}`: source parameter is missing",
            template.name
        ))?;

        let source = match template.kind {
            OffchainDataSourceKind::Ipfs => match source.parse() {
                Ok(source) => Source::Ipfs(source),
                // Ignore data sources created with an invalid CID.
                Err(e) => return Err(DataSourceCreationError::Ignore(source, e)),
            },
            OffchainDataSourceKind::Arweave => Source::Arweave(Word::from(source)),
        };

        Ok(Self {
            kind: template.kind.clone(),
            name: template.name.clone(),
            manifest_idx: template.manifest_idx,
            source,
            mapping: template.mapping,
            context: Arc::new(info.context),
            creation_block: Some(info.creation_block),
            done_at: Arc::new(AtomicI32::new(NOT_DONE_VALUE)),
            causality_region,
        })
    }

    pub fn match_and_decode<C: Blockchain>(
        &self,
        trigger: &TriggerData,
    ) -> Option<TriggerWithHandler<super::MappingTrigger<C>>> {
        if self.source != trigger.source || self.is_processed() {
            return None;
        }
        Some(TriggerWithHandler::new(
            data_source::MappingTrigger::Offchain(trigger.clone()),
            self.mapping.handler.clone(),
            BlockPtr::new(Default::default(), self.creation_block.unwrap_or(0)),
        ))
    }

    pub fn as_stored_dynamic_data_source(&self) -> StoredDynamicDataSource {
        let param = self.source.clone().into();
        let done_at = self.done_at.load(std::sync::atomic::Ordering::SeqCst);
        let done_at = if done_at == NOT_DONE_VALUE {
            None
        } else {
            Some(done_at)
        };

        let context = self
            .context
            .as_ref()
            .as_ref()
            .map(|ctx| serde_json::to_value(ctx).unwrap());

        StoredDynamicDataSource {
            manifest_idx: self.manifest_idx,
            param: Some(param),
            context,
            creation_block: self.creation_block,
            done_at,
            causality_region: self.causality_region,
        }
    }

    pub fn from_stored_dynamic_data_source(
        template: &DataSourceTemplate,
        stored: StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        let StoredDynamicDataSource {
            manifest_idx,
            param,
            context,
            creation_block,
            done_at,
            causality_region,
        } = stored;

        let param = param.context("no param on stored data source")?;
        let source = template.kind.try_parse_source(param)?;
        let context = Arc::new(context.map(serde_json::from_value).transpose()?);

        Ok(Self {
            kind: template.kind.clone(),
            name: template.name.clone(),
            manifest_idx,
            source,
            mapping: template.mapping.clone(),
            context,
            creation_block,
            done_at: Arc::new(AtomicI32::new(done_at.unwrap_or(NOT_DONE_VALUE))),
            causality_region,
        })
    }

    pub fn address(&self) -> Option<Vec<u8>> {
        self.source.address()
    }

    pub(super) fn is_duplicate_of(&self, b: &DataSource) -> bool {
        let DataSource {
            // Inferred from the manifest_idx
            kind: _,
            name: _,
            mapping: _,

            manifest_idx,
            source,
            context,

            // We want to deduplicate across done status or creation block.
            done_at: _,
            creation_block: _,

            // The causality region is also ignored, to be able to detect duplicated file data
            // sources.
            //
            // Note to future: This will become more complicated if we allow for example file data
            // sources to create other file data sources, because which one is created first (the
            // original) and which is created later (the duplicate) is no longer deterministic. One
            // fix would be to check the equality of the parent causality region.
            causality_region: _,
        } = self;

        // See also: data-source-is-duplicate-of
        manifest_idx == &b.manifest_idx && source == &b.source && context == &b.context
    }
}

pub type Base64 = Word;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    Ipfs(CidFile),
    Arweave(Base64),
}

impl Source {
    /// The concept of an address may or not make sense for an offchain data source, but graph node
    /// will use this in a few places where some sort of not necessarily unique id is useful:
    /// 1. This is used as the value to be returned to mappings from the `dataSource.address()` host
    ///    function, so changing this is a breaking change.
    /// 2. This is used to match with triggers with hosts in `fn hosts_for_trigger`, so make sure
    ///    the `source` of the data source is equal the `source` of the `TriggerData`.
    pub fn address(&self) -> Option<Vec<u8>> {
        match self {
            Source::Ipfs(ref cid) => Some(cid.to_bytes()),
            Source::Arweave(ref base64) => Some(base64.as_bytes().to_vec()),
        }
    }
}

impl Into<Bytes> for Source {
    fn into(self) -> Bytes {
        match self {
            Source::Ipfs(ref link) => Bytes::from(link.to_bytes()),
            Source::Arweave(ref base64) => Bytes::from(base64.as_bytes()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub language: String,
    pub api_version: semver::Version,
    pub entities: Vec<EntityType>,
    pub handler: String,
    pub runtime: Arc<Vec<u8>>,
    pub link: Link,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub name: String,
    pub source: UnresolvedSource,
    pub mapping: UnresolvedMapping,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct UnresolvedSource {
    file: Link,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    pub api_version: String,
    pub language: String,
    pub file: Link,
    pub handler: String,
    pub entities: Vec<String>,
}

impl UnresolvedDataSource {
    #[allow(dead_code)]
    pub(super) async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        manifest_idx: u32,
        causality_region: CausalityRegion,
        schema: &InputSchema,
    ) -> Result<DataSource, Error> {
        info!(logger, "Resolve offchain data source";
            "name" => &self.name,
            "kind" => &self.kind,
            "source" => format_args!("{:?}", &self.source),
        );

        let kind = OffchainDataSourceKind::from_str(self.kind.as_str())?;
        let source = kind.try_parse_source(Bytes::from(self.source.file.link.as_bytes()))?;

        Ok(DataSource {
            manifest_idx,
            kind,
            name: self.name,
            source,
            mapping: self.mapping.resolve(resolver, schema, logger).await?,
            context: Arc::new(None),
            creation_block: None,
            done_at: Arc::new(AtomicI32::new(NOT_DONE_VALUE)),
            causality_region,
        })
    }
}

impl UnresolvedMapping {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        schema: &InputSchema,
        logger: &Logger,
    ) -> Result<Mapping, Error> {
        info!(logger, "Resolve offchain mapping"; "link" => &self.file.link);
        // It is possible for a manifest to mention entity types that do not
        // exist in the schema. Rather than fail the subgraph, which could
        // fail existing subgraphs, filter them out and just log a warning.
        let (entities, errs) = self
            .entities
            .iter()
            .map(|s| schema.entity_type(s).map_err(|_| s))
            .partition::<Vec<_>, _>(Result::is_ok);
        if !errs.is_empty() {
            let errs = errs.into_iter().map(Result::unwrap_err).join(", ");
            warn!(logger, "Ignoring unknown entity types in mapping"; "entities" => errs, "link" => &self.file.link);
        }
        let entities = entities.into_iter().map(Result::unwrap).collect::<Vec<_>>();
        Ok(Mapping {
            language: self.language,
            api_version: semver::Version::parse(&self.api_version)?,
            entities,
            handler: self.handler,
            runtime: Arc::new(resolver.cat(logger, &self.file).await?),
            link: self.file,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnresolvedDataSourceTemplate {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub mapping: UnresolvedMapping,
}

#[derive(Clone, Debug)]
pub struct DataSourceTemplate {
    pub kind: OffchainDataSourceKind,
    pub network: Option<String>,
    pub name: String,
    pub manifest_idx: u32,
    pub mapping: Mapping,
}

impl UnresolvedDataSourceTemplate {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        manifest_idx: u32,
        schema: &InputSchema,
    ) -> Result<DataSourceTemplate, Error> {
        let kind = OffchainDataSourceKind::from_str(&self.kind)?;

        let mapping = self
            .mapping
            .resolve(resolver, schema, logger)
            .await
            .with_context(|| format!("failed to resolve data source template {}", self.name))?;

        Ok(DataSourceTemplate {
            kind,
            network: self.network,
            name: self.name,
            manifest_idx,
            mapping,
        })
    }
}

#[derive(Clone)]
pub struct TriggerData {
    pub source: Source,
    pub data: Arc<bytes::Bytes>,
}

impl fmt::Debug for TriggerData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[derive(Debug)]
        struct TriggerDataWithoutData<'a> {
            _source: &'a Source,
        }
        write!(
            f,
            "{:?}",
            TriggerDataWithoutData {
                _source: &self.source
            }
        )
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::{
        data::{store::scalar::Bytes, value::Word},
        ipfs_client::CidFile,
    };

    use super::{OffchainDataSourceKind, Source};

    #[test]
    fn test_source_bytes_round_trip() {
        let base64 = "8APeQ5lW0-csTcBaGdPBDLAL2ci2AT9pTn2tppGPU_8";
        let cid = CidFile::from_str("QmVkvoPGi9jvvuxsHDVJDgzPEzagBaWSZRYoRDzU244HjZ").unwrap();

        let ipfs_source: Bytes = Source::Ipfs(cid.clone()).into();
        let s = OffchainDataSourceKind::Ipfs
            .try_parse_source(ipfs_source)
            .unwrap();
        assert! { matches!(s, Source::Ipfs(ipfs) if ipfs.eq(&cid))};

        let arweave_source = Source::Arweave(Word::from(base64));
        let s = OffchainDataSourceKind::Arweave
            .try_parse_source(arweave_source.into())
            .unwrap();
        assert! { matches!(s, Source::Arweave(b64) if b64.eq(&base64))};
    }
}
