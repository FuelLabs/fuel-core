use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, Context, Error};
use graph::{
    blockchain,
    cheap_clone::CheapClone,
    components::link_resolver::LinkResolver,
    prelude::{async_trait, BlockNumber, DataSourceTemplateInfo, Link},
    slog::Logger,
};

use prost::Message;
use serde::Deserialize;

use crate::{chain::Chain, Block, TriggerData};

pub const SUBSTREAMS_KIND: &str = "substreams";

const DYNAMIC_DATA_SOURCE_ERROR: &str = "Substreams do not support dynamic data sources";
const TEMPLATE_ERROR: &str = "Substreams do not support templates";

const ALLOWED_MAPPING_KIND: [&str; 1] = ["substreams/graph-entities"];
const SUBSTREAMS_HANDLER_KIND: &str = "substreams";
#[derive(Clone, Debug, PartialEq)]
/// Represents the DataSource portion of the manifest once it has been parsed
/// and the substream spkg has been downloaded + parsed.
pub struct DataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: Source,
    pub mapping: Mapping,
    pub context: Arc<Option<graph::prelude::DataSourceContext>>,
    pub initial_block: Option<BlockNumber>,
}

impl blockchain::DataSource<Chain> for DataSource {
    fn from_template_info(_template_info: DataSourceTemplateInfo<Chain>) -> Result<Self, Error> {
        Err(anyhow!("Substreams does not support templates"))
    }

    fn address(&self) -> Option<&[u8]> {
        None
    }

    fn start_block(&self) -> BlockNumber {
        self.initial_block.unwrap_or(0)
    }

    fn end_block(&self) -> Option<BlockNumber> {
        None
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

    fn context(&self) -> Arc<Option<graph::prelude::DataSourceContext>> {
        self.context.cheap_clone()
    }

    fn creation_block(&self) -> Option<BlockNumber> {
        None
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        self.mapping.handler.as_ref().map(|h| h.runtime.clone())
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        // This is placeholder, substreams do not have a handler kind.
        vec![SUBSTREAMS_HANDLER_KIND].into_iter().collect()
    }

    // match_and_decode only seems to be used on the default trigger processor which substreams
    // bypasses so it should be fine to leave it unimplemented.
    fn match_and_decode(
        &self,
        _trigger: &TriggerData,
        _block: &Arc<Block>,
        _logger: &Logger,
    ) -> Result<Option<blockchain::TriggerWithHandler<Chain>>, Error> {
        unimplemented!()
    }

    fn is_duplicate_of(&self, _other: &Self) -> bool {
        todo!()
    }

    fn as_stored_dynamic_data_source(&self) -> graph::components::store::StoredDynamicDataSource {
        unimplemented!("{}", DYNAMIC_DATA_SOURCE_ERROR)
    }

    fn validate(&self) -> Vec<Error> {
        let mut errs = vec![];

        if &self.kind != SUBSTREAMS_KIND {
            errs.push(anyhow!(
                "data source has invalid `kind`, expected {} but found {}",
                SUBSTREAMS_KIND,
                self.kind
            ))
        }

        if self.name.is_empty() {
            errs.push(anyhow!("name cannot be empty"));
        }

        if !ALLOWED_MAPPING_KIND.contains(&self.mapping.kind.as_str()) {
            errs.push(anyhow!(
                "mapping kind has to be one of {:?}, found {}",
                ALLOWED_MAPPING_KIND,
                self.mapping.kind
            ))
        }

        errs
    }

    fn from_stored_dynamic_data_source(
        _template: &<Chain as blockchain::Blockchain>::DataSourceTemplate,
        _stored: graph::components::store::StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        Err(anyhow!(DYNAMIC_DATA_SOURCE_ERROR))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
/// Module name comes from the manifest, package is the parsed spkg file.
pub struct Source {
    pub module_name: String,
    pub package: graph::substreams::Package,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub api_version: semver::Version,
    pub kind: String,
    pub handler: Option<MappingHandler>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingHandler {
    pub handler: String,
    pub runtime: Arc<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
/// Raw representation of the data source for deserialization purposes.
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub(crate) source: UnresolvedSource,
    pub mapping: UnresolvedMapping,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Text api_version, before parsing and validation.
pub struct UnresolvedMapping {
    pub api_version: String,
    pub kind: String,
    pub handler: Option<String>,
    pub file: Option<Link>,
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<DataSource, Error> {
        let content = resolver.cat(logger, &self.source.package.file).await?;

        let mut package = graph::substreams::Package::decode(content.as_ref())?;

        let module = match package.modules.as_mut() {
            Some(modules) => modules
                .modules
                .iter_mut()
                .find(|module| module.name == self.source.package.module_name)
                .map(|module| {
                    if let Some(params) = self.source.package.params {
                        graph::substreams::patch_module_params(params, module);
                    }
                    module
                }),
            None => None,
        };

        let initial_block: Option<u64> = match module {
            Some(module) => match &module.kind {
                Some(graph::substreams::module::Kind::KindMap(_)) => Some(module.initial_block),
                _ => {
                    return Err(anyhow!(
                        "Substreams module {} must be of 'map' kind",
                        module.name
                    ))
                }
            },
            None => {
                return Err(anyhow!(
                    "Substreams module {} does not exist",
                    self.source.package.module_name
                ))
            }
        };

        let initial_block: Option<i32> = initial_block
            .map_or(Ok(None), |x: u64| TryInto::<i32>::try_into(x).map(Some))
            .map_err(anyhow::Error::from)?;

        let handler = match (self.mapping.handler, self.mapping.file) {
            (Some(handler), Some(file)) => {
                let module_bytes = resolver
                    .cat(logger, &file)
                    .await
                    .with_context(|| format!("failed to resolve mapping {}", file.link))?;

                Some(MappingHandler {
                    handler,
                    runtime: Arc::new(module_bytes),
                })
            }
            _ => None,
        };

        Ok(DataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: self.network,
            name: self.name,
            source: Source {
                module_name: self.source.package.module_name,
                package,
            },
            mapping: Mapping {
                api_version: semver::Version::parse(&self.mapping.api_version)?,
                kind: self.mapping.kind,
                handler,
            },
            context: Arc::new(None),
            initial_block,
        })
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Source is a part of the manifest and this is needed for parsing.
pub struct UnresolvedSource {
    #[serde(rename = "startBlock", default)]
    start_block: Option<BlockNumber>,
    package: UnresolvedPackage,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
/// The unresolved Package section of the manifest.
pub struct UnresolvedPackage {
    pub module_name: String,
    pub file: Link,
    pub params: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
/// This is necessary for the Blockchain trait associated types, substreams do not support
/// data source templates so this is a noop and is not expected to be called.
pub struct NoopDataSourceTemplate {}

impl blockchain::DataSourceTemplate<Chain> for NoopDataSourceTemplate {
    fn name(&self) -> &str {
        unimplemented!("{}", TEMPLATE_ERROR);
    }

    fn api_version(&self) -> semver::Version {
        unimplemented!("{}", TEMPLATE_ERROR);
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        unimplemented!("{}", TEMPLATE_ERROR);
    }

    fn manifest_idx(&self) -> u32 {
        todo!()
    }

    fn kind(&self) -> &str {
        unimplemented!("{}", TEMPLATE_ERROR);
    }
}

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for NoopDataSourceTemplate {
    async fn resolve(
        self,
        _resolver: &Arc<dyn LinkResolver>,
        _logger: &Logger,
        _manifest_idx: u32,
    ) -> Result<NoopDataSourceTemplate, anyhow::Error> {
        unimplemented!("{}", TEMPLATE_ERROR)
    }
}

#[cfg(test)]
mod test {
    use std::{str::FromStr, sync::Arc};

    use anyhow::Error;
    use graph::{
        blockchain::{DataSource as _, UnresolvedDataSource as _},
        components::link_resolver::LinkResolver,
        prelude::{async_trait, serde_yaml, JsonValueStream, Link},
        slog::{o, Discard, Logger},
        substreams::module::{Kind, KindMap, KindStore},
        substreams::{
            module::input::{Input, Params},
            Module, Modules, Package,
        },
    };
    use prost::Message;

    use crate::{DataSource, Mapping, UnresolvedDataSource, UnresolvedMapping, SUBSTREAMS_KIND};

    #[test]
    fn parse_data_source() {
        let ds: UnresolvedDataSource = serde_yaml::from_str(TEMPLATE_DATA_SOURCE).unwrap();
        let expected = UnresolvedDataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::UnresolvedSource {
                package: crate::UnresolvedPackage {
                    module_name: "output".into(),
                    file: Link {
                        link: "/ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT".into(),
                    },
                    params: None,
                },
                start_block: None,
            },
            mapping: UnresolvedMapping {
                api_version: "0.0.7".into(),
                kind: "substreams/graph-entities".into(),
                handler: None,
                file: None,
            },
        };
        assert_eq!(ds, expected);
    }

    #[test]
    fn parse_data_source_with_params() {
        let ds: UnresolvedDataSource =
            serde_yaml::from_str(TEMPLATE_DATA_SOURCE_WITH_PARAMS).unwrap();
        let expected = UnresolvedDataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::UnresolvedSource {
                package: crate::UnresolvedPackage {
                    module_name: "output".into(),
                    file: Link {
                        link: "/ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT".into(),
                    },
                    params: Some("x\ny\n123\n".into()),
                },
                start_block: None,
            },
            mapping: UnresolvedMapping {
                api_version: "0.0.7".into(),
                kind: "substreams/graph-entities".into(),
                handler: None,
                file: None,
            },
        };
        assert_eq!(ds, expected);
    }

    #[tokio::test]
    async fn data_source_conversion() {
        let ds: UnresolvedDataSource = serde_yaml::from_str(TEMPLATE_DATA_SOURCE).unwrap();
        let link_resolver: Arc<dyn LinkResolver> = Arc::new(NoopLinkResolver {});
        let logger = Logger::root(Discard, o!());
        let ds: DataSource = ds.resolve(&link_resolver, &logger, 0).await.unwrap();
        let expected = DataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::Source {
                module_name: "output".into(),
                package: gen_package(),
            },
            mapping: Mapping {
                api_version: semver::Version::from_str("0.0.7").unwrap(),
                kind: "substreams/graph-entities".into(),
                handler: None,
            },
            context: Arc::new(None),
            initial_block: Some(123),
        };
        assert_eq!(ds, expected);
    }

    #[tokio::test]
    async fn data_source_conversion_override_params() {
        let mut package = gen_package();
        let mut modules = package.modules.unwrap();
        modules.modules.get_mut(0).map(|module| {
            module.inputs = vec![graph::substreams::module::Input {
                input: Some(Input::Params(Params {
                    value: "x\ny\n123\n".into(),
                })),
            }]
        });
        package.modules = Some(modules);

        let ds: UnresolvedDataSource =
            serde_yaml::from_str(TEMPLATE_DATA_SOURCE_WITH_PARAMS).unwrap();
        let link_resolver: Arc<dyn LinkResolver> = Arc::new(NoopLinkResolver {});
        let logger = Logger::root(Discard, o!());
        let ds: DataSource = ds.resolve(&link_resolver, &logger, 0).await.unwrap();
        let expected = DataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::Source {
                module_name: "output".into(),
                package,
            },
            mapping: Mapping {
                api_version: semver::Version::from_str("0.0.7").unwrap(),
                kind: "substreams/graph-entities".into(),
                handler: None,
            },
            context: Arc::new(None),
            initial_block: Some(123),
        };
        assert_eq!(ds, expected);
    }

    #[test]
    fn data_source_validation() {
        let mut ds = gen_data_source();
        assert_eq!(true, ds.validate().is_empty());

        ds.network = None;
        assert_eq!(true, ds.validate().is_empty());

        ds.kind = "asdasd".into();
        ds.name = "".into();
        ds.mapping.kind = "asdasd".into();
        let errs: Vec<String> = ds.validate().into_iter().map(|e| e.to_string()).collect();
        assert_eq!(
            errs,
            vec![
                "data source has invalid `kind`, expected substreams but found asdasd",
                "name cannot be empty",
                "mapping kind has to be one of [\"substreams/graph-entities\"], found asdasd"
            ]
        );
    }

    #[test]
    fn parse_data_source_with_maping() {
        let ds: UnresolvedDataSource =
            serde_yaml::from_str(TEMPLATE_DATA_SOURCE_WITH_MAPPING).unwrap();

        let expected = UnresolvedDataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::UnresolvedSource {
                package: crate::UnresolvedPackage {
                    module_name: "output".into(),
                    file: Link {
                        link: "/ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT".into(),
                    },
                    params: Some("x\ny\n123\n".into()),
                },
                start_block: None,
            },
            mapping: UnresolvedMapping {
                api_version: "0.0.7".into(),
                kind: "substreams/graph-entities".into(),
                handler: Some("bananas".to_string()),
                file: Some(Link {
                    link: "./src/mappings.ts".to_string(),
                }),
            },
        };
        assert_eq!(ds, expected);
    }

    fn gen_package() -> Package {
        Package {
            proto_files: vec![],
            version: 0,
            modules: Some(Modules {
                modules: vec![
                    Module {
                        name: "output".into(),
                        initial_block: 123,
                        binary_entrypoint: "output".into(),
                        binary_index: 0,
                        kind: Some(Kind::KindMap(KindMap {
                            output_type: "proto".into(),
                        })),
                        inputs: vec![],
                        output: None,
                    },
                    Module {
                        name: "store_mod".into(),
                        initial_block: 0,
                        binary_entrypoint: "store_mod".into(),
                        binary_index: 0,
                        kind: Some(Kind::KindStore(KindStore {
                            update_policy: 1,
                            value_type: "proto1".into(),
                        })),
                        inputs: vec![],
                        output: None,
                    },
                    Module {
                        name: "map_mod".into(),
                        initial_block: 123456,
                        binary_entrypoint: "other2".into(),
                        binary_index: 0,
                        kind: Some(Kind::KindMap(KindMap {
                            output_type: "proto2".into(),
                        })),
                        inputs: vec![],
                        output: None,
                    },
                ],
                binaries: vec![],
            }),
            module_meta: vec![],
            package_meta: vec![],
            sink_config: None,
            network: "".into(),
            sink_module: "".into(),
        }
    }

    fn gen_data_source() -> DataSource {
        DataSource {
            kind: SUBSTREAMS_KIND.into(),
            network: Some("mainnet".into()),
            name: "Uniswap".into(),
            source: crate::Source {
                module_name: "".to_string(),
                package: gen_package(),
            },
            mapping: Mapping {
                api_version: semver::Version::from_str("0.0.7").unwrap(),
                kind: "substreams/graph-entities".into(),
                handler: None,
            },
            context: Arc::new(None),
            initial_block: None,
        }
    }

    const TEMPLATE_DATA_SOURCE: &str = r#"
        kind: substreams
        name: Uniswap
        network: mainnet
        source:
          package:
            moduleName: output
            file:
              /: /ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT
              # This IPFs path would be generated from a local path at deploy time
        mapping:
          kind: substreams/graph-entities
          apiVersion: 0.0.7
    "#;

    const TEMPLATE_DATA_SOURCE_WITH_MAPPING: &str = r#"
        kind: substreams
        name: Uniswap
        network: mainnet
        source:
          package:
            moduleName: output
            file:
              /: /ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT
              # This IPFs path would be generated from a local path at deploy time
            params: |
                x
                y
                123
        mapping:
          kind: substreams/graph-entities
          apiVersion: 0.0.7
          file:
            /: ./src/mappings.ts
          handler: bananas
    "#;

    const TEMPLATE_DATA_SOURCE_WITH_PARAMS: &str = r#"
        kind: substreams
        name: Uniswap
        network: mainnet
        source:
          package:
            moduleName: output
            file:
              /: /ipfs/QmbHnhUFZa6qqqRyubUYhXntox1TCBxqryaBM1iNGqVJzT
              # This IPFs path would be generated from a local path at deploy time
            params: |
                x
                y
                123
        mapping:
          kind: substreams/graph-entities
          apiVersion: 0.0.7
    "#;

    #[derive(Debug)]
    struct NoopLinkResolver {}

    #[async_trait]
    impl LinkResolver for NoopLinkResolver {
        fn with_timeout(&self, _timeout: std::time::Duration) -> Box<dyn LinkResolver> {
            unimplemented!()
        }

        fn with_retries(&self) -> Box<dyn LinkResolver> {
            unimplemented!()
        }

        async fn cat(&self, _logger: &Logger, _link: &Link) -> Result<Vec<u8>, Error> {
            Ok(gen_package().encode_to_vec())
        }

        async fn get_block(&self, _logger: &Logger, _link: &Link) -> Result<Vec<u8>, Error> {
            unimplemented!()
        }

        async fn json_stream(
            &self,
            _logger: &Logger,
            _link: &Link,
        ) -> Result<JsonValueStream, Error> {
            unimplemented!()
        }
    }
}
