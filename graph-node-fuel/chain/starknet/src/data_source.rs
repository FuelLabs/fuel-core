use graph::{
    anyhow::{anyhow, Error},
    blockchain::{self, Block as BlockchainBlock, TriggerWithHandler},
    components::{link_resolver::LinkResolver, store::StoredDynamicDataSource},
    data::subgraph::DataSourceContext,
    prelude::{async_trait, BlockNumber, DataSourceTemplateInfo, Deserialize, Link, Logger},
    semver,
};
use sha3::{Digest, Keccak256};
use starknet_ff::FieldElement;
use std::{collections::HashSet, sync::Arc};

use crate::{
    chain::Chain,
    codec,
    trigger::{StarknetEventTrigger, StarknetTrigger},
};

const BLOCK_HANDLER_KIND: &str = "block";
const EVENT_HANDLER_KIND: &str = "event";

#[derive(Clone)]
pub struct DataSource {
    pub kind: String,
    pub network: String,
    pub name: String,
    pub source: Source,
    pub mapping: Mapping,
}

#[derive(Clone)]
pub struct Mapping {
    pub block_handlers: Vec<MappingBlockHandler>,
    pub event_handlers: Vec<MappingEventHandler>,
    pub runtime: Arc<Vec<u8>>,
}

#[derive(Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: String,
    pub name: String,
    pub source: Source,
    pub mapping: UnresolvedMapping,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub start_block: BlockNumber,
    pub end_block: Option<BlockNumber>,
    #[serde(default, deserialize_with = "deserialize_address")]
    pub address: Option<FieldElement>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    #[serde(default)]
    pub block_handlers: Vec<MappingBlockHandler>,
    #[serde(default)]
    pub event_handlers: Vec<UnresolvedMappingEventHandler>,
    pub file: Link,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct MappingBlockHandler {
    pub handler: String,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct MappingEventHandler {
    pub handler: String,
    pub event_selector: FieldElement,
}

#[derive(Clone, Deserialize)]
pub struct UnresolvedMappingEventHandler {
    pub handler: String,
    pub event: String,
}

#[derive(Debug, Clone)]
pub struct DataSourceTemplate;

#[derive(Clone, Default, Deserialize)]
pub struct UnresolvedDataSourceTemplate;

impl blockchain::DataSource<Chain> for DataSource {
    fn from_template_info(_template_info: DataSourceTemplateInfo<Chain>) -> Result<Self, Error> {
        Err(anyhow!("StarkNet subgraphs do not support templates"))
    }

    fn address(&self) -> Option<&[u8]> {
        None
    }

    fn start_block(&self) -> BlockNumber {
        self.source.start_block
    }

    fn end_block(&self) -> Option<BlockNumber> {
        self.source.end_block
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        let mut kinds = HashSet::new();

        let Mapping {
            block_handlers,
            event_handlers,
            ..
        } = &self.mapping;

        if !block_handlers.is_empty() {
            kinds.insert(BLOCK_HANDLER_KIND);
        }
        if !event_handlers.is_empty() {
            kinds.insert(EVENT_HANDLER_KIND);
        }

        kinds
    }

    fn match_and_decode(
        &self,
        trigger: &StarknetTrigger,
        block: &Arc<codec::Block>,
        _logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<Chain>>, Error> {
        if self.start_block() > block.number() {
            return Ok(None);
        }

        let handler = match trigger {
            StarknetTrigger::Block(_) => match self.mapping.block_handlers.first() {
                Some(handler) => handler.handler.clone(),
                None => return Ok(None),
            },
            StarknetTrigger::Event(event) => match self.handler_for_event(event) {
                Some(handler) => handler.handler,
                None => return Ok(None),
            },
        };

        Ok(Some(TriggerWithHandler::<Chain>::new(
            trigger.clone(),
            handler,
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
        Some(&self.network)
    }

    fn context(&self) -> Arc<Option<DataSourceContext>> {
        Arc::new(None)
    }

    fn creation_block(&self) -> Option<BlockNumber> {
        None
    }

    fn is_duplicate_of(&self, other: &Self) -> bool {
        let DataSource {
            kind,
            network,
            name,
            source,
            mapping,
        } = self;

        kind == &other.kind
            && network == &other.network
            && name == &other.name
            && source == &other.source
            && mapping.event_handlers == other.mapping.event_handlers
            && mapping.block_handlers == other.mapping.block_handlers
    }

    fn as_stored_dynamic_data_source(&self) -> StoredDynamicDataSource {
        // FIXME (Starknet): Implement me!
        todo!()
    }

    fn from_stored_dynamic_data_source(
        _template: &DataSourceTemplate,
        _stored: StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        // FIXME (Starknet): Implement me correctly
        todo!()
    }

    fn validate(&self) -> Vec<Error> {
        Default::default()
    }

    fn api_version(&self) -> semver::Version {
        semver::Version::new(0, 0, 5)
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.clone())
    }
}

impl DataSource {
    /// Returns event trigger if an event.key matches the handler.key and optionally
    /// if event.fromAddr matches the source address. Note this only supports the default
    /// Starknet behavior of one key per event.
    fn handler_for_event(&self, event: &StarknetEventTrigger) -> Option<MappingEventHandler> {
        let event_key = FieldElement::from_byte_slice_be(event.event.keys.first()?).ok()?;

        // Always deocding first here seems fine as we expect most sources to define an address
        // filter anyways. Alternatively we can use lazy init here, which seems unnecessary.
        let event_from_addr = FieldElement::from_byte_slice_be(&event.event.from_addr).ok()?;

        return self
            .mapping
            .event_handlers
            .iter()
            .find(|handler| {
                // No need to compare address if selector doesn't match
                if handler.event_selector != event_key {
                    return false;
                }

                match &self.source.address {
                    Some(addr_filter) => addr_filter == &event_from_addr,
                    None => true,
                }
            })
            .cloned();
    }
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSource, Error> {
        let module_bytes = resolver.cat(logger, &self.mapping.file).await?;

        Ok(DataSource {
            kind: self.kind,
            network: self.network,
            name: self.name,
            source: self.source,
            mapping: Mapping {
                block_handlers: self.mapping.block_handlers,
                event_handlers: self
                    .mapping
                    .event_handlers
                    .into_iter()
                    .map(|handler| {
                        Ok(MappingEventHandler {
                            handler: handler.handler,
                            event_selector: get_selector_from_name(&handler.event)?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
                runtime: Arc::new(module_bytes),
            },
        })
    }
}

impl blockchain::DataSourceTemplate<Chain> for DataSourceTemplate {
    fn api_version(&self) -> semver::Version {
        todo!()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        todo!()
    }

    fn name(&self) -> &str {
        todo!()
    }

    fn manifest_idx(&self) -> u32 {
        todo!()
    }

    fn kind(&self) -> &str {
        todo!()
    }
}

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for UnresolvedDataSourceTemplate {
    #[allow(unused)]
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        manifest_idx: u32,
    ) -> Result<DataSourceTemplate, Error> {
        todo!()
    }
}

// Adapted from:
//   https://github.com/xJonathanLEI/starknet-rs/blob/f16271877c9dbf08bc7bf61e4fc72decc13ff73d/starknet-core/src/utils.rs#L110-L121
fn get_selector_from_name(func_name: &str) -> graph::anyhow::Result<FieldElement> {
    const DEFAULT_ENTRY_POINT_NAME: &str = "__default__";
    const DEFAULT_L1_ENTRY_POINT_NAME: &str = "__l1_default__";

    if func_name == DEFAULT_ENTRY_POINT_NAME || func_name == DEFAULT_L1_ENTRY_POINT_NAME {
        Ok(FieldElement::ZERO)
    } else {
        let name_bytes = func_name.as_bytes();
        if name_bytes.is_ascii() {
            Ok(starknet_keccak(name_bytes))
        } else {
            Err(anyhow!("the provided name contains non-ASCII characters"))
        }
    }
}

// Adapted from:
//   https://github.com/xJonathanLEI/starknet-rs/blob/f16271877c9dbf08bc7bf61e4fc72decc13ff73d/starknet-core/src/utils.rs#L98-L108
fn starknet_keccak(data: &[u8]) -> FieldElement {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    let mut hash = hasher.finalize();

    // Remove the first 6 bits
    hash[0] &= 0b00000011;

    // Because we know hash is always 32 bytes
    FieldElement::from_bytes_be(unsafe { &*(hash[..].as_ptr() as *const [u8; 32]) }).unwrap()
}

fn deserialize_address<'de, D>(deserializer: D) -> Result<Option<FieldElement>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    Ok(Some(serde::Deserialize::deserialize(deserializer)?))
}
