use graph::anyhow::Context;
use graph::blockchain::{Block, TriggerWithHandler};
use graph::components::store::StoredDynamicDataSource;
use graph::data::subgraph::DataSourceContext;
use graph::prelude::SubgraphManifestValidationError;
use graph::{
    anyhow::{anyhow, Error},
    blockchain::{self, Blockchain},
    prelude::{
        async_trait, BlockNumber, CheapClone, DataSourceTemplateInfo, Deserialize, Link,
        LinkResolver, Logger,
    },
    semver,
};
use std::collections::HashSet;
use std::sync::Arc;

use crate::chain::Chain;
use crate::trigger::{NearTrigger, ReceiptWithOutcome};

pub const NEAR_KIND: &str = "near";
const BLOCK_HANDLER_KIND: &str = "block";
const RECEIPT_HANDLER_KIND: &str = "receipt";

/// Runtime representation of a data source.
#[derive(Clone, Debug)]
pub struct DataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: Source,
    pub mapping: Mapping,
    pub context: Arc<Option<DataSourceContext>>,
    pub creation_block: Option<BlockNumber>,
}

impl blockchain::DataSource<Chain> for DataSource {
    fn from_template_info(_template_info: DataSourceTemplateInfo<Chain>) -> Result<Self, Error> {
        Err(anyhow!("Near subgraphs do not support templates"))

        // How this might be implemented if/when Near gets support for templates:
        // let DataSourceTemplateInfo {
        //     template,
        //     params,
        //     context,
        //     creation_block,
        // } = info;

        // let account = params
        //     .get(0)
        //     .with_context(|| {
        //         format!(
        //             "Failed to create data source from template `{}`: account parameter is missing",
        //             template.name
        //         )
        //     })?
        //     .clone();

        // Ok(DataSource {
        //     kind: template.kind,
        //     network: template.network,
        //     name: template.name,
        //     source: Source {
        //         account,
        //         start_block: 0,
        //     },
        //     mapping: template.mapping,
        //     context: Arc::new(context),
        //     creation_block: Some(creation_block),
        // })
    }

    fn address(&self) -> Option<&[u8]> {
        self.source.account.as_ref().map(String::as_bytes)
    }

    fn start_block(&self) -> BlockNumber {
        self.source.start_block
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        let mut kinds = HashSet::new();

        if self.handler_for_block().is_some() {
            kinds.insert(BLOCK_HANDLER_KIND);
        }

        if self.handler_for_receipt().is_some() {
            kinds.insert(RECEIPT_HANDLER_KIND);
        }

        kinds
    }

    fn end_block(&self) -> Option<BlockNumber> {
        self.source.end_block
    }

    fn match_and_decode(
        &self,
        trigger: &<Chain as Blockchain>::TriggerData,
        block: &Arc<<Chain as Blockchain>::Block>,
        _logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<Chain>>, Error> {
        if self.source.start_block > block.number() {
            return Ok(None);
        }

        fn account_matches(ds: &DataSource, receipt: &Arc<ReceiptWithOutcome>) -> bool {
            if Some(&receipt.receipt.receiver_id) == ds.source.account.as_ref() {
                return true;
            }

            if let Some(partial_accounts) = &ds.source.accounts {
                let matches_prefix = if partial_accounts.prefixes.is_empty() {
                    true
                } else {
                    partial_accounts
                        .prefixes
                        .iter()
                        .any(|prefix| receipt.receipt.receiver_id.starts_with(prefix))
                };

                let matches_suffix = if partial_accounts.suffixes.is_empty() {
                    true
                } else {
                    partial_accounts
                        .suffixes
                        .iter()
                        .any(|suffix| receipt.receipt.receiver_id.ends_with(suffix))
                };

                if matches_prefix && matches_suffix {
                    return true;
                }
            }

            false
        }

        let handler = match trigger {
            // A block trigger matches if a block handler is present.
            NearTrigger::Block(_) => match self.handler_for_block() {
                Some(handler) => &handler.handler,
                None => return Ok(None),
            },

            // A receipt trigger matches if the receiver matches `source.account` and a receipt
            // handler is present.
            NearTrigger::Receipt(receipt) => {
                if !account_matches(self, receipt) {
                    return Ok(None);
                }

                match self.handler_for_receipt() {
                    Some(handler) => &handler.handler,
                    None => return Ok(None),
                }
            }
        };

        Ok(Some(TriggerWithHandler::<Chain>::new(
            trigger.cheap_clone(),
            handler.clone(),
            block.ptr(),
        )))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    fn context(&self) -> Arc<Option<DataSourceContext>> {
        self.context.cheap_clone()
    }

    fn creation_block(&self) -> Option<BlockNumber> {
        self.creation_block
    }

    fn is_duplicate_of(&self, other: &Self) -> bool {
        let DataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context,

            // The creation block is ignored for detection duplicate data sources.
            // Contract ABI equality is implicit in `source` and `mapping.abis` equality.
            creation_block: _,
        } = self;

        // mapping_request_sender, host_metrics, and (most of) host_exports are operational structs
        // used at runtime but not needed to define uniqueness; each runtime host should be for a
        // unique data source.
        kind == &other.kind
            && network == &other.network
            && name == &other.name
            && source == &other.source
            && mapping.block_handlers == other.mapping.block_handlers
            && context == &other.context
    }

    fn as_stored_dynamic_data_source(&self) -> StoredDynamicDataSource {
        // FIXME (NEAR): Implement me!
        todo!()
    }

    fn from_stored_dynamic_data_source(
        _template: &DataSourceTemplate,
        _stored: StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        // FIXME (NEAR): Implement me correctly
        todo!()
    }

    fn validate(&self) -> Vec<Error> {
        let mut errors = Vec::new();

        if self.kind != NEAR_KIND {
            errors.push(anyhow!(
                "data source has invalid `kind`, expected {} but found {}",
                NEAR_KIND,
                self.kind
            ))
        }

        // Validate that there is a `source` address if there are receipt handlers
        let no_source_address = self.address().is_none();

        // Validate that there are no empty PartialAccount.
        let no_partial_addresses = match &self.source.accounts {
            None => true,
            Some(addrs) => addrs.is_empty(),
        };

        let has_receipt_handlers = !self.mapping.receipt_handlers.is_empty();

        // Validate not both address and partial addresses are empty.
        if (no_source_address && no_partial_addresses) && has_receipt_handlers {
            errors.push(SubgraphManifestValidationError::SourceAddressRequired.into());
        };

        // Validate empty lines not allowed in suffix or prefix
        if let Some(partial_accounts) = self.source.accounts.as_ref() {
            if partial_accounts.prefixes.iter().any(|x| x.is_empty()) {
                errors.push(anyhow!("partial account prefixes can't have empty values"))
            }

            if partial_accounts.suffixes.iter().any(|x| x.is_empty()) {
                errors.push(anyhow!("partial account suffixes can't have empty values"))
            }
        }

        // Validate that there are no more than one of both block handlers and receipt handlers
        if self.mapping.block_handlers.len() > 1 {
            errors.push(anyhow!("data source has duplicated block handlers"));
        }
        if self.mapping.receipt_handlers.len() > 1 {
            errors.push(anyhow!("data source has duplicated receipt handlers"));
        }

        errors
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }
}

impl DataSource {
    fn from_manifest(
        kind: String,
        network: Option<String>,
        name: String,
        source: Source,
        mapping: Mapping,
        context: Option<DataSourceContext>,
    ) -> Result<Self, Error> {
        // Data sources in the manifest are created "before genesis" so they have no creation block.
        let creation_block = None;

        Ok(DataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context: Arc::new(context),
            creation_block,
        })
    }

    fn handler_for_block(&self) -> Option<&MappingBlockHandler> {
        self.mapping.block_handlers.first()
    }

    fn handler_for_receipt(&self) -> Option<&ReceiptHandler> {
        self.mapping.receipt_handlers.first()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: Source,
    pub mapping: UnresolvedMapping,
    pub context: Option<DataSourceContext>,
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSource, Error> {
        let UnresolvedDataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context,
        } = self;

        let mapping = mapping.resolve(resolver, logger).await.with_context(|| {
            format!(
                "failed to resolve data source {} with source_account {:?} and source_start_block {}",
                name, source.account, source.start_block
            )
        })?;

        DataSource::from_manifest(kind, network, name, source, mapping, context)
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct BaseDataSourceTemplate<M> {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub mapping: M,
}

pub type UnresolvedDataSourceTemplate = BaseDataSourceTemplate<UnresolvedMapping>;
pub type DataSourceTemplate = BaseDataSourceTemplate<Mapping>;

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for UnresolvedDataSourceTemplate {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSourceTemplate, Error> {
        let UnresolvedDataSourceTemplate {
            kind,
            network,
            name,
            mapping,
        } = self;

        let mapping = mapping
            .resolve(resolver, logger)
            .await
            .with_context(|| format!("failed to resolve data source template {}", name))?;

        Ok(DataSourceTemplate {
            kind,
            network,
            name,
            mapping,
        })
    }
}

impl blockchain::DataSourceTemplate<Chain> for DataSourceTemplate {
    fn name(&self) -> &str {
        &self.name
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }

    fn manifest_idx(&self) -> u32 {
        unreachable!("near does not support dynamic data sources")
    }

    fn kind(&self) -> &str {
        &self.kind
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    pub api_version: String,
    pub language: String,
    pub entities: Vec<String>,
    #[serde(default)]
    pub block_handlers: Vec<MappingBlockHandler>,
    #[serde(default)]
    pub receipt_handlers: Vec<ReceiptHandler>,
    pub file: Link,
}

impl UnresolvedMapping {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
    ) -> Result<Mapping, Error> {
        let UnresolvedMapping {
            api_version,
            language,
            entities,
            block_handlers,
            receipt_handlers,
            file: link,
        } = self;

        let api_version = semver::Version::parse(&api_version)?;

        let module_bytes = resolver
            .cat(logger, &link)
            .await
            .with_context(|| format!("failed to resolve mapping {}", link.link))?;

        Ok(Mapping {
            api_version,
            language,
            entities,
            block_handlers,
            receipt_handlers,
            runtime: Arc::new(module_bytes),
            link,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub api_version: semver::Version,
    pub language: String,
    pub entities: Vec<String>,
    pub block_handlers: Vec<MappingBlockHandler>,
    pub receipt_handlers: Vec<ReceiptHandler>,
    pub runtime: Arc<Vec<u8>>,
    pub link: Link,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingBlockHandler {
    pub handler: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct ReceiptHandler {
    pub(crate) handler: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize, Default)]
pub(crate) struct PartialAccounts {
    #[serde(default)]
    pub(crate) prefixes: Vec<String>,
    #[serde(default)]
    pub(crate) suffixes: Vec<String>,
}

impl PartialAccounts {
    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty() && self.suffixes.is_empty()
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Source {
    // A data source that does not have an account or accounts can only have block handlers.
    pub(crate) account: Option<String>,
    #[serde(default)]
    pub(crate) start_block: BlockNumber,
    pub(crate) end_block: Option<BlockNumber>,
    pub(crate) accounts: Option<PartialAccounts>,
}
