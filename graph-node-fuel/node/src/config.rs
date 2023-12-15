use graph::{
    anyhow::Error,
    blockchain::BlockchainKind,
    env::ENV_VARS,
    firehose::{SubgraphLimit, SUBGRAPHS_PER_CONN},
    itertools::Itertools,
    prelude::{
        anyhow::{anyhow, bail, Context, Result},
        info,
        regex::Regex,
        serde::{
            de::{self, value, SeqAccess, Visitor},
            Deserialize, Deserializer, Serialize,
        },
        serde_json, serde_regex, toml, Logger, NodeId, StoreError,
    },
};
use graph_chain_ethereum::{self as ethereum, NodeCapabilities};
use graph_store_postgres::{DeploymentPlacer, Shard as ShardName, PRIMARY_SHARD};

use http::{HeaderMap, Uri};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use std::{fs::read_to_string, time::Duration};
use url::Url;

const ANY_NAME: &str = ".*";
/// A regular expression that matches nothing
const NO_NAME: &str = ".^";

pub struct Opt {
    pub postgres_url: Option<String>,
    pub config: Option<String>,
    // This is only used when we cosntruct a config purely from command
    // line options. When using a configuration file, pool sizes must be
    // set in the configuration file alone
    pub store_connection_pool_size: u32,
    pub postgres_secondary_hosts: Vec<String>,
    pub postgres_host_weights: Vec<usize>,
    pub disable_block_ingestor: bool,
    pub node_id: String,
    pub ethereum_rpc: Vec<String>,
    pub ethereum_ws: Vec<String>,
    pub ethereum_ipc: Vec<String>,
    pub unsafe_config: bool,
}

impl Default for Opt {
    fn default() -> Self {
        Opt {
            postgres_url: None,
            config: None,
            store_connection_pool_size: 10,
            postgres_secondary_hosts: vec![],
            postgres_host_weights: vec![],
            disable_block_ingestor: true,
            node_id: "default".to_string(),
            ethereum_rpc: vec![],
            ethereum_ws: vec![],
            ethereum_ipc: vec![],
            unsafe_config: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(skip, default = "default_node_id")]
    pub node: NodeId,
    pub general: Option<GeneralSection>,
    #[serde(rename = "store")]
    pub stores: BTreeMap<String, Shard>,
    pub chains: ChainSection,
    pub deployment: Deployment,
}

fn validate_name(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(anyhow!("names must not be empty"));
    }
    if s.len() > 30 {
        return Err(anyhow!(
            "names can be at most 30 characters, but `{}` has {} characters",
            s,
            s.len()
        ));
    }

    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(anyhow!(
            "name `{}` is invalid: names can only contain lowercase alphanumeric characters or '-'",
            s
        ));
    }
    Ok(())
}

impl Config {
    /// Check that the config is valid.
    fn validate(&mut self) -> Result<()> {
        if !self.stores.contains_key(PRIMARY_SHARD.as_str()) {
            return Err(anyhow!("missing a primary store"));
        }
        if self.stores.len() > 1 && ethereum::ENV_VARS.cleanup_blocks {
            // See 8b6ad0c64e244023ac20ced7897fe666
            return Err(anyhow!(
                "GRAPH_ETHEREUM_CLEANUP_BLOCKS can not be used with a sharded store"
            ));
        }
        for (key, shard) in self.stores.iter_mut() {
            shard.validate(key)?;
        }
        self.deployment.validate()?;

        // Check that deployment rules only reference existing stores and chains
        for (i, rule) in self.deployment.rules.iter().enumerate() {
            for shard in &rule.shards {
                if !self.stores.contains_key(shard) {
                    return Err(anyhow!("unknown shard {} in deployment rule {}", shard, i));
                }
            }
            if let Some(networks) = &rule.pred.network {
                for network in networks.to_vec() {
                    if !self.chains.chains.contains_key(&network) {
                        return Err(anyhow!(
                            "unknown network {} in deployment rule {}",
                            network,
                            i
                        ));
                    }
                }
            }
        }

        // Check that chains only reference existing stores
        for (name, chain) in &self.chains.chains {
            if !self.stores.contains_key(&chain.shard) {
                return Err(anyhow!("unknown shard {} in chain {}", chain.shard, name));
            }
        }

        self.chains.validate()?;

        Ok(())
    }

    /// Load a configuration file if `opt.config` is set. If not, generate
    /// a config from the command line arguments in `opt`
    pub fn load(logger: &Logger, opt: &Opt) -> Result<Config> {
        if let Some(config) = &opt.config {
            Self::from_file(logger, config, &opt.node_id)
        } else {
            info!(
                logger,
                "Generating configuration from command line arguments"
            );
            Self::from_opt(opt)
        }
    }

    pub fn from_file(logger: &Logger, path: &str, node: &str) -> Result<Config> {
        info!(logger, "Reading configuration file `{}`", path);
        Self::from_str(&read_to_string(path)?, node)
    }

    pub fn from_str(config: &str, node: &str) -> Result<Config> {
        let mut config: Config = toml::from_str(config)?;
        config.node = NodeId::new(node).map_err(|()| anyhow!("invalid node id {}", node))?;
        config.validate()?;
        Ok(config)
    }

    fn from_opt(opt: &Opt) -> Result<Config> {
        let deployment = Deployment::from_opt(opt);
        let mut stores = BTreeMap::new();
        let chains = ChainSection::from_opt(opt)?;
        let node = NodeId::new(opt.node_id.to_string())
            .map_err(|()| anyhow!("invalid node id {}", opt.node_id))?;
        stores.insert(PRIMARY_SHARD.to_string(), Shard::from_opt(true, opt)?);
        Ok(Config {
            node,
            general: None,
            stores,
            chains,
            deployment,
        })
    }

    /// Generate a JSON representation of the config.
    pub fn to_json(&self) -> Result<String> {
        // It would be nice to produce a TOML representation, but that runs
        // into this error: https://github.com/alexcrichton/toml-rs/issues/142
        // and fixing it as described in the issue didn't fix it. Since serializing
        // this data isn't crucial and only needed for debugging, we'll
        // just stick with JSON
        Ok(serde_json::to_string_pretty(&self)?)
    }

    pub fn primary_store(&self) -> &Shard {
        self.stores
            .get(PRIMARY_SHARD.as_str())
            .expect("a validated config has a primary store")
    }

    pub fn query_only(&self, node: &NodeId) -> bool {
        self.general
            .as_ref()
            .map(|g| match g.query.find(node.as_str()) {
                None => false,
                Some(m) => m.as_str() == node.as_str(),
            })
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeneralSection {
    #[serde(with = "serde_regex", default = "no_name")]
    query: Regex,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Shard {
    pub connection: String,
    #[serde(default = "one")]
    pub weight: usize,
    #[serde(default)]
    pub pool_size: PoolSize,
    #[serde(default = "PoolSize::five")]
    pub fdw_pool_size: PoolSize,
    #[serde(default)]
    pub replicas: BTreeMap<String, Replica>,
}

impl Shard {
    fn validate(&mut self, name: &str) -> Result<()> {
        ShardName::new(name.to_string()).map_err(|e| anyhow!(e))?;

        self.connection = shellexpand::env(&self.connection)?.into_owned();

        if matches!(self.pool_size, PoolSize::None) {
            return Err(anyhow!("missing pool size definition for shard `{}`", name));
        }

        self.pool_size
            .validate(name == PRIMARY_SHARD.as_str(), &self.connection)?;
        for (name, replica) in self.replicas.iter_mut() {
            validate_name(name).context("illegal replica name")?;
            replica.validate(name == PRIMARY_SHARD.as_str(), &self.pool_size)?;
        }

        let no_weight =
            self.weight == 0 && self.replicas.values().all(|replica| replica.weight == 0);
        if no_weight {
            return Err(anyhow!(
                "all weights for shard `{}` are 0; \
                remove explicit weights or set at least one of them to a value bigger than 0",
                name
            ));
        }
        Ok(())
    }

    fn from_opt(is_primary: bool, opt: &Opt) -> Result<Self> {
        let postgres_url = opt
            .postgres_url
            .as_ref()
            .expect("validation checked that postgres_url is set");
        let pool_size = PoolSize::Fixed(opt.store_connection_pool_size);
        pool_size.validate(is_primary, postgres_url)?;
        let mut replicas = BTreeMap::new();
        for (i, host) in opt.postgres_secondary_hosts.iter().enumerate() {
            let replica = Replica {
                connection: replace_host(postgres_url, host),
                weight: opt.postgres_host_weights.get(i + 1).cloned().unwrap_or(1),
                pool_size: pool_size.clone(),
            };
            replicas.insert(format!("replica{}", i + 1), replica);
        }
        Ok(Self {
            connection: postgres_url.clone(),
            weight: opt.postgres_host_weights.first().cloned().unwrap_or(1),
            pool_size,
            fdw_pool_size: PoolSize::five(),
            replicas,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PoolSize {
    None,
    Fixed(u32),
    Rule(Vec<PoolSizeRule>),
}

impl Default for PoolSize {
    fn default() -> Self {
        Self::None
    }
}

impl PoolSize {
    fn five() -> Self {
        Self::Fixed(5)
    }

    fn validate(&self, is_primary: bool, connection: &str) -> Result<()> {
        use PoolSize::*;

        let pool_size = match self {
            None => bail!("missing pool size for {}", connection),
            Fixed(s) => *s,
            Rule(rules) => rules.iter().map(|rule| rule.size).min().unwrap_or(0u32),
        };

        match pool_size {
            0 if is_primary => Err(anyhow!(
                "the pool size for the primary shard must be at least 2"
            )),
            0 => Ok(()),
            1 => Err(anyhow!(
                "connection pool size must be at least 2, but is {} for {}",
                pool_size,
                connection
            )),
            _ => Ok(()),
        }
    }

    pub fn size_for(&self, node: &NodeId, name: &str) -> Result<u32> {
        use PoolSize::*;
        match self {
            None => unreachable!("validation ensures we have a pool size"),
            Fixed(s) => Ok(*s),
            Rule(rules) => rules
                .iter()
                .find(|rule| rule.matches(node.as_str()))
                .map(|rule| rule.size)
                .ok_or_else(|| {
                    anyhow!(
                        "no rule matches node id `{}` for the pool of shard {}",
                        node.as_str(),
                        name
                    )
                }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PoolSizeRule {
    #[serde(with = "serde_regex", default = "any_name")]
    node: Regex,
    size: u32,
}

impl PoolSizeRule {
    fn matches(&self, name: &str) -> bool {
        match self.node.find(name) {
            None => false,
            Some(m) => m.as_str() == name,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Replica {
    pub connection: String,
    #[serde(default = "one")]
    pub weight: usize,
    #[serde(default)]
    pub pool_size: PoolSize,
}

impl Replica {
    fn validate(&mut self, is_primary: bool, pool_size: &PoolSize) -> Result<()> {
        self.connection = shellexpand::env(&self.connection)?.into_owned();
        if matches!(self.pool_size, PoolSize::None) {
            self.pool_size = pool_size.clone();
        }

        self.pool_size.validate(is_primary, &self.connection)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainSection {
    pub ingestor: String,
    #[serde(flatten)]
    pub chains: BTreeMap<String, Chain>,
}

impl ChainSection {
    fn validate(&mut self) -> Result<()> {
        NodeId::new(&self.ingestor)
            .map_err(|()| anyhow!("invalid node id for ingestor {}", &self.ingestor))?;
        for (_, chain) in self.chains.iter_mut() {
            chain.validate()?
        }
        Ok(())
    }

    fn from_opt(opt: &Opt) -> Result<Self> {
        // If we are not the block ingestor, set the node name
        // to something that is definitely not our node_id
        let ingestor = if opt.disable_block_ingestor {
            format!("{} is not ingesting", opt.node_id)
        } else {
            opt.node_id.clone()
        };
        let mut chains = BTreeMap::new();
        Self::parse_networks(&mut chains, Transport::Rpc, &opt.ethereum_rpc)?;
        Self::parse_networks(&mut chains, Transport::Ws, &opt.ethereum_ws)?;
        Self::parse_networks(&mut chains, Transport::Ipc, &opt.ethereum_ipc)?;
        Ok(Self { ingestor, chains })
    }

    pub fn providers(&self) -> Vec<String> {
        self.chains
            .values()
            .flat_map(|chain| {
                chain
                    .providers
                    .iter()
                    .map(|p| p.label.clone())
                    .collect::<Vec<String>>()
            })
            .collect()
    }

    fn parse_networks(
        chains: &mut BTreeMap<String, Chain>,
        transport: Transport,
        args: &Vec<String>,
    ) -> Result<()> {
        for (nr, arg) in args.iter().enumerate() {
            if arg.starts_with("wss://")
                || arg.starts_with("http://")
                || arg.starts_with("https://")
            {
                return Err(anyhow!(
                    "Is your Ethereum node string missing a network name? \
                     Try 'mainnet:' + the Ethereum node URL."
                ));
            } else {
                // Parse string (format is "NETWORK_NAME:NETWORK_CAPABILITIES:URL" OR
                // "NETWORK_NAME::URL" which will default to NETWORK_CAPABILITIES="archive,traces")
                let colon = arg.find(':').ok_or_else(|| {
                    anyhow!(
                        "A network name must be provided alongside the \
                         Ethereum node location. Try e.g. 'mainnet:URL'."
                    )
                })?;

                let (name, rest_with_delim) = arg.split_at(colon);
                let rest = &rest_with_delim[1..];
                if name.is_empty() {
                    return Err(anyhow!("Ethereum network name cannot be an empty string"));
                }
                if rest.is_empty() {
                    return Err(anyhow!("Ethereum node URL cannot be an empty string"));
                }

                let colon = rest.find(':').ok_or_else(|| {
                    anyhow!(
                        "A network name must be provided alongside the \
                         Ethereum node location. Try e.g. 'mainnet:URL'."
                    )
                })?;

                let (features, url_str) = rest.split_at(colon);
                let (url, features) = if vec!["http", "https", "ws", "wss"].contains(&features) {
                    (rest, DEFAULT_PROVIDER_FEATURES.to_vec())
                } else {
                    (&url_str[1..], features.split(',').collect())
                };
                let features = features.into_iter().map(|s| s.to_string()).collect();
                let provider = Provider {
                    label: format!("{}-{}-{}", name, transport, nr),
                    details: ProviderDetails::Web3(Web3Provider {
                        transport,
                        url: url.to_string(),
                        features,
                        headers: Default::default(),
                        rules: vec![],
                    }),
                };
                let entry = chains.entry(name.to_string()).or_insert_with(|| Chain {
                    shard: PRIMARY_SHARD.to_string(),
                    protocol: BlockchainKind::Ethereum,
                    polling_interval: default_polling_interval(),
                    providers: vec![],
                });
                entry.providers.push(provider);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Chain {
    pub shard: String,
    #[serde(default = "default_blockchain_kind")]
    pub protocol: BlockchainKind,
    #[serde(
        default = "default_polling_interval",
        deserialize_with = "deserialize_duration_millis"
    )]
    pub polling_interval: Duration,
    #[serde(rename = "provider")]
    pub providers: Vec<Provider>,
}

fn default_blockchain_kind() -> BlockchainKind {
    BlockchainKind::Ethereum
}

impl Chain {
    fn validate(&mut self) -> Result<()> {
        let mut labels = self.providers.iter().map(|p| &p.label).collect_vec();
        labels.sort();
        labels.dedup();
        if labels.len() != self.providers.len() {
            return Err(anyhow!("Provider labels must be unique"));
        }

        // `Config` validates that `self.shard` references a configured shard
        for provider in self.providers.iter_mut() {
            provider.validate()?
        }

        if !matches!(self.protocol, BlockchainKind::Substreams) {
            let has_only_substreams_providers = self
                .providers
                .iter()
                .all(|provider| matches!(provider.details, ProviderDetails::Substreams(_)));
            if has_only_substreams_providers {
                bail!(
                    "{} protocol requires an rpc or firehose endpoint defined",
                    self.protocol
                );
            }
        }

        // When using substreams protocol, only substreams endpoints are allowed
        if matches!(self.protocol, BlockchainKind::Substreams) {
            let has_non_substreams_providers = self
                .providers
                .iter()
                .any(|provider| !matches!(provider.details, ProviderDetails::Substreams(_)));
            if has_non_substreams_providers {
                bail!("Substreams protocol only supports substreams providers");
            }
        }

        Ok(())
    }
}

fn deserialize_http_headers<'de, D>(deserializer: D) -> Result<HeaderMap, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let kvs: BTreeMap<String, String> = Deserialize::deserialize(deserializer)?;
    Ok(btree_map_to_http_headers(kvs))
}

fn btree_map_to_http_headers(kvs: BTreeMap<String, String>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (k, v) in kvs.into_iter() {
        headers.insert(
            k.parse::<http::header::HeaderName>()
                .unwrap_or_else(|_| panic!("invalid HTTP header name: {}", k)),
            v.parse::<http::header::HeaderValue>()
                .unwrap_or_else(|_| panic!("invalid HTTP header value: {}: {}", k, v)),
        );
    }
    headers
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Provider {
    pub label: String,
    pub details: ProviderDetails,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderDetails {
    Firehose(FirehoseProvider),
    Web3(Web3Provider),
    Substreams(FirehoseProvider),
    Web3Call(Web3Provider),
}

const FIREHOSE_FILTER_FEATURE: &str = "filters";
const FIREHOSE_COMPRESSION_FEATURE: &str = "compression";
const FIREHOSE_PROVIDER_FEATURES: [&str; 2] =
    [FIREHOSE_FILTER_FEATURE, FIREHOSE_COMPRESSION_FEATURE];

fn twenty() -> u16 {
    20
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct FirehoseProvider {
    pub url: String,
    pub token: Option<String>,
    #[serde(default = "twenty")]
    pub conn_pool_size: u16,
    #[serde(default)]
    pub features: BTreeSet<String>,
    #[serde(default, rename = "match")]
    rules: Vec<Web3Rule>,
}

impl FirehoseProvider {
    pub fn limit_for(&self, node: &NodeId) -> SubgraphLimit {
        self.rules.limit_for(node)
    }
    pub fn filters_enabled(&self) -> bool {
        self.features.contains(FIREHOSE_FILTER_FEATURE)
    }
    pub fn compression_enabled(&self) -> bool {
        self.features.contains(FIREHOSE_COMPRESSION_FEATURE)
    }
}

pub trait Web3Rules {
    fn limit_for(&self, node: &NodeId) -> SubgraphLimit;
}

impl Web3Rules for Vec<Web3Rule> {
    fn limit_for(&self, node: &NodeId) -> SubgraphLimit {
        self.iter()
            .map(|rule| rule.limit_for(node))
            .max()
            .unwrap_or(SubgraphLimit::Unlimited)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Web3Rule {
    #[serde(with = "serde_regex")]
    name: Regex,
    limit: usize,
}

impl PartialEq for Web3Rule {
    fn eq(&self, other: &Self) -> bool {
        self.name.to_string() == other.name.to_string() && self.limit == other.limit
    }
}

impl Web3Rule {
    fn limit_for(&self, node: &NodeId) -> SubgraphLimit {
        match self.name.find(node.as_str()) {
            Some(m) if m.as_str() == node.as_str() => {
                if self.limit == 0 {
                    SubgraphLimit::Disabled
                } else {
                    SubgraphLimit::Limit(self.limit)
                }
            }
            _ => SubgraphLimit::Disabled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Web3Provider {
    #[serde(default)]
    pub transport: Transport,
    pub url: String,
    pub features: BTreeSet<String>,

    // TODO: This should be serialized.
    #[serde(
        skip_serializing,
        default,
        deserialize_with = "deserialize_http_headers"
    )]
    pub headers: HeaderMap,

    #[serde(default, rename = "match")]
    rules: Vec<Web3Rule>,
}

impl Web3Provider {
    pub fn node_capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {
            archive: self.features.contains("archive"),
            traces: self.features.contains("traces"),
        }
    }

    pub fn limit_for(&self, node: &NodeId) -> SubgraphLimit {
        self.rules.limit_for(node)
    }
}

const PROVIDER_FEATURES: [&str; 3] = ["traces", "archive", "no_eip1898"];
const DEFAULT_PROVIDER_FEATURES: [&str; 2] = ["traces", "archive"];

impl Provider {
    fn validate(&mut self) -> Result<()> {
        validate_name(&self.label).context("illegal provider name")?;

        match self.details {
            ProviderDetails::Firehose(ref mut firehose)
            | ProviderDetails::Substreams(ref mut firehose) => {
                firehose.url = shellexpand::env(&firehose.url)?.into_owned();

                // A Firehose url must be a valid Uri since gRPC library we use (Tonic)
                // works with Uri.
                let label = &self.label;
                firehose.url.parse::<Uri>().map_err(|e| {
                    anyhow!(
                        "the url `{}` for firehose provider {} is not a legal URI: {}",
                        firehose.url,
                        label,
                        e
                    )
                })?;

                if let Some(token) = &firehose.token {
                    firehose.token = Some(shellexpand::env(token)?.into_owned());
                }

                if firehose
                    .features
                    .iter()
                    .any(|feature| !FIREHOSE_PROVIDER_FEATURES.contains(&feature.as_str()))
                {
                    return Err(anyhow!(
                        "supported firehose endpoint filters are: {:?}",
                        FIREHOSE_PROVIDER_FEATURES
                    ));
                }

                if firehose.rules.iter().any(|r| r.limit > SUBGRAPHS_PER_CONN) {
                    bail!(
                        "per node subgraph limit for firehose/substreams has to be in the range 0-{}",
                        SUBGRAPHS_PER_CONN
                    );
                }
            }

            ProviderDetails::Web3Call(ref mut web3) | ProviderDetails::Web3(ref mut web3) => {
                for feature in &web3.features {
                    if !PROVIDER_FEATURES.contains(&feature.as_str()) {
                        return Err(anyhow!(
                            "illegal feature `{}` for provider {}. Features must be one of {}",
                            feature,
                            self.label,
                            PROVIDER_FEATURES.join(", ")
                        ));
                    }
                }

                web3.url = shellexpand::env(&web3.url)?.into_owned();

                let label = &self.label;
                Url::parse(&web3.url).map_err(|e| {
                    anyhow!(
                        "the url `{}` for provider {} is not a legal URL: {}",
                        web3.url,
                        label,
                        e
                    )
                })?;
            }
        }

        Ok(())
    }
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ProviderVisitor;

        impl<'de> serde::de::Visitor<'de> for ProviderVisitor {
            type Value = Provider;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Provider")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Provider, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut label = None;
                let mut details = None;

                let mut url = None;
                let mut transport = None;
                let mut features = None;
                let mut headers = None;
                let mut nodes = Vec::new();

                while let Some(key) = map.next_key()? {
                    match key {
                        ProviderField::Label => {
                            if label.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label = Some(map.next_value()?);
                        }
                        ProviderField::Details => {
                            if details.is_some() {
                                return Err(serde::de::Error::duplicate_field("details"));
                            }
                            details = Some(map.next_value()?);
                        }
                        ProviderField::Url => {
                            if url.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url = Some(map.next_value()?);
                        }
                        ProviderField::Transport => {
                            if transport.is_some() {
                                return Err(serde::de::Error::duplicate_field("transport"));
                            }
                            transport = Some(map.next_value()?);
                        }
                        ProviderField::Features => {
                            if features.is_some() {
                                return Err(serde::de::Error::duplicate_field("features"));
                            }
                            features = Some(map.next_value()?);
                        }
                        ProviderField::Headers => {
                            if headers.is_some() {
                                return Err(serde::de::Error::duplicate_field("headers"));
                            }

                            let raw_headers: BTreeMap<String, String> = map.next_value()?;
                            headers = Some(btree_map_to_http_headers(raw_headers));
                        }
                        ProviderField::Match => {
                            nodes = map.next_value()?;
                        }
                    }
                }

                let label = label.ok_or_else(|| serde::de::Error::missing_field("label"))?;
                let details = match details {
                    Some(mut v) => {
                        if url.is_some()
                            || transport.is_some()
                            || features.is_some()
                            || headers.is_some()
                        {
                            return Err(serde::de::Error::custom("when `details` field is provided, deprecated `url`, `transport`, `features` and `headers` cannot be specified"));
                        }

                        match v {
                            ProviderDetails::Firehose(ref mut firehose)
                            | ProviderDetails::Substreams(ref mut firehose) => {
                                firehose.rules = nodes
                            }
                            _ => {}
                        }

                        v
                    }
                    None => ProviderDetails::Web3(Web3Provider {
                        url: url.ok_or_else(|| serde::de::Error::missing_field("url"))?,
                        transport: transport.unwrap_or(Transport::Rpc),
                        features: features
                            .ok_or_else(|| serde::de::Error::missing_field("features"))?,
                        headers: headers.unwrap_or_else(HeaderMap::new),
                        rules: nodes,
                    }),
                };

                Ok(Provider { label, details })
            }
        }

        const FIELDS: &[&str] = &[
            "label",
            "details",
            "transport",
            "url",
            "features",
            "headers",
        ];
        deserializer.deserialize_struct("Provider", FIELDS, ProviderVisitor)
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum ProviderField {
    Label,
    Details,
    Match,

    // Deprecated fields
    Url,
    Transport,
    Features,
    Headers,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Transport {
    #[serde(rename = "rpc")]
    Rpc,
    #[serde(rename = "ws")]
    Ws,
    #[serde(rename = "ipc")]
    Ipc,
}

impl Default for Transport {
    fn default() -> Self {
        Self::Rpc
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Transport::*;

        match self {
            Rpc => write!(f, "rpc"),
            Ws => write!(f, "ws"),
            Ipc => write!(f, "ipc"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Deployment {
    #[serde(rename = "rule")]
    rules: Vec<Rule>,
}

impl Deployment {
    fn validate(&self) -> Result<()> {
        if self.rules.is_empty() {
            return Err(anyhow!(
                "there must be at least one deployment rule".to_string()
            ));
        }
        let mut default_rule = false;
        for rule in &self.rules {
            rule.validate()?;
            if default_rule {
                return Err(anyhow!("rules after a default rule are useless"));
            }
            default_rule = rule.is_default();
        }
        if !default_rule {
            return Err(anyhow!(
                "the rules do not contain a default rule that matches everything"
            ));
        }
        Ok(())
    }

    fn from_opt(_: &Opt) -> Self {
        Self { rules: vec![] }
    }
}

impl DeploymentPlacer for Deployment {
    fn place(
        &self,
        name: &str,
        network: &str,
    ) -> Result<Option<(Vec<ShardName>, Vec<NodeId>)>, String> {
        // Errors here are really programming errors. We should have validated
        // everything already so that the various conversions can't fail. We
        // still return errors so that they bubble up to the deployment request
        // rather than crashing the node and burying the crash in the logs
        let placement = match self.rules.iter().find(|rule| rule.matches(name, network)) {
            Some(rule) => {
                let shards = rule.shard_names().map_err(|e| e.to_string())?;
                let indexers: Vec<_> = rule
                    .indexers
                    .iter()
                    .map(|idx| {
                        NodeId::new(idx.clone())
                            .map_err(|()| format!("{} is not a valid node name", idx))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Some((shards, indexers))
            }
            None => None,
        };
        Ok(placement)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Rule {
    #[serde(rename = "match", default)]
    pred: Predicate,
    // For backwards compatibility, we also accept 'shard' for the shards
    #[serde(
        alias = "shard",
        default = "primary_store",
        deserialize_with = "string_or_vec"
    )]
    shards: Vec<String>,
    indexers: Vec<String>,
}

impl Rule {
    fn is_default(&self) -> bool {
        self.pred.matches_anything()
    }

    fn matches(&self, name: &str, network: &str) -> bool {
        self.pred.matches(name, network)
    }

    fn shard_names(&self) -> Result<Vec<ShardName>, StoreError> {
        self.shards
            .iter()
            .cloned()
            .map(ShardName::new)
            .collect::<Result<_, _>>()
    }

    fn validate(&self) -> Result<()> {
        if self.indexers.is_empty() {
            return Err(anyhow!("useless rule without indexers"));
        }
        for indexer in &self.indexers {
            NodeId::new(indexer).map_err(|()| anyhow!("invalid node id {}", &indexer))?;
        }
        self.shard_names().map_err(Error::from)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Predicate {
    #[serde(with = "serde_regex", default = "any_name")]
    name: Regex,
    network: Option<NetworkPredicate>,
}

impl Predicate {
    fn matches_anything(&self) -> bool {
        self.name.as_str() == ANY_NAME && self.network.is_none()
    }

    pub fn matches(&self, name: &str, network: &str) -> bool {
        if let Some(n) = &self.network {
            if !n.matches(network) {
                return false;
            }
        }

        match self.name.find(name) {
            None => false,
            Some(m) => m.as_str() == name,
        }
    }
}

impl Default for Predicate {
    fn default() -> Self {
        Predicate {
            name: any_name(),
            network: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum NetworkPredicate {
    Single(String),
    Many(Vec<String>),
}

impl NetworkPredicate {
    fn matches(&self, network: &str) -> bool {
        use NetworkPredicate::*;
        match self {
            Single(n) => n == network,
            Many(ns) => ns.iter().any(|n| n == network),
        }
    }

    fn to_vec(&self) -> Vec<String> {
        use NetworkPredicate::*;
        match self {
            Single(n) => vec![n.clone()],
            Many(ns) => ns.clone(),
        }
    }
}

/// Replace the host portion of `url` and return a new URL with `host`
/// as the host portion
///
/// Panics if `url` is not a valid URL (which won't happen in our case since
/// we would have paniced before getting here as `url` is the connection for
/// the primary Postgres instance)
fn replace_host(url: &str, host: &str) -> String {
    let mut url = match Url::parse(url) {
        Ok(url) => url,
        Err(_) => panic!("Invalid Postgres URL {}", url),
    };
    if let Err(e) = url.set_host(Some(host)) {
        panic!("Invalid Postgres url {}: {}", url, e);
    }
    String::from(url)
}

// Various default functions for deserialization
fn any_name() -> Regex {
    Regex::new(ANY_NAME).unwrap()
}

fn no_name() -> Regex {
    Regex::new(NO_NAME).unwrap()
}

fn primary_store() -> Vec<String> {
    vec![PRIMARY_SHARD.to_string()]
}

fn one() -> usize {
    1
}

fn default_node_id() -> NodeId {
    NodeId::new("default").unwrap()
}

fn default_polling_interval() -> Duration {
    ENV_VARS.ingestor_polling_interval
}

fn deserialize_duration_millis<'de, D>(data: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = u64::deserialize(data)?;
    Ok(Duration::from_millis(millis))
}

// From https://github.com/serde-rs/serde/issues/889#issuecomment-295988865
fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![s.to_owned()])
        }

        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: SeqAccess<'de>,
        {
            Deserialize::deserialize(value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

#[cfg(test)]
mod tests {

    use crate::config::{default_polling_interval, ChainSection, Web3Rule};

    use super::{
        Chain, Config, FirehoseProvider, Provider, ProviderDetails, Transport, Web3Provider,
    };
    use graph::blockchain::BlockchainKind;
    use graph::firehose::SubgraphLimit;
    use graph::prelude::regex::Regex;
    use graph::prelude::{toml, NodeId};
    use http::{HeaderMap, HeaderValue};
    use std::collections::BTreeSet;
    use std::fs::read_to_string;
    use std::path::{Path, PathBuf};

    #[test]
    fn it_works_on_standard_config() {
        let content = read_resource_as_string("full_config.toml");
        let actual: Config = toml::from_str(&content).unwrap();

        // We do basic checks because writing the full equality method is really too long

        assert_eq!(
            "query_node_.*".to_string(),
            actual.general.unwrap().query.to_string()
        );
        assert_eq!(4, actual.chains.chains.len());
        assert_eq!(2, actual.stores.len());
        assert_eq!(3, actual.deployment.rules.len());
    }

    #[test]
    fn it_works_on_chain_without_protocol() {
        let actual = toml::from_str(
            r#"
            shard = "primary"
            provider = []
        "#,
        )
        .unwrap();

        assert_eq!(
            Chain {
                shard: "primary".to_string(),
                protocol: BlockchainKind::Ethereum,
                polling_interval: default_polling_interval(),
                providers: vec![],
            },
            actual
        );
    }

    #[test]
    fn it_works_on_chain_with_protocol() {
        let actual = toml::from_str(
            r#"
            shard = "primary"
            protocol = "near"
            provider = []
        "#,
        )
        .unwrap();

        assert_eq!(
            Chain {
                shard: "primary".to_string(),
                protocol: BlockchainKind::Near,
                polling_interval: default_polling_interval(),
                providers: vec![],
            },
            actual
        );
    }

    #[test]
    fn it_works_on_deprecated_provider_from_toml() {
        let actual = toml::from_str(
            r#"
            transport = "rpc"
            label = "peering"
            url = "http://localhost:8545"
            features = []
        "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "peering".to_owned(),
                details: ProviderDetails::Web3(Web3Provider {
                    transport: Transport::Rpc,
                    url: "http://localhost:8545".to_owned(),
                    features: BTreeSet::new(),
                    headers: HeaderMap::new(),
                    rules: Vec::new(),
                }),
            },
            actual
        );
    }

    #[test]
    fn it_works_on_deprecated_provider_without_transport_from_toml() {
        let actual = toml::from_str(
            r#"
            label = "peering"
            url = "http://localhost:8545"
            features = []
        "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "peering".to_owned(),
                details: ProviderDetails::Web3(Web3Provider {
                    transport: Transport::Rpc,
                    url: "http://localhost:8545".to_owned(),
                    features: BTreeSet::new(),
                    headers: HeaderMap::new(),
                    rules: Vec::new(),
                }),
            },
            actual
        );
    }

    #[test]
    fn it_errors_on_deprecated_provider_missing_url_from_toml() {
        let actual = toml::from_str::<Provider>(
            r#"
            transport = "rpc"
            label = "peering"
            features = []
        "#,
        );

        assert_eq!(true, actual.is_err());
        let err_str = actual.unwrap_err().to_string();
        assert_eq!(err_str.contains("missing field `url`"), true, "{}", err_str);
    }

    #[test]
    fn it_errors_on_deprecated_provider_missing_features_from_toml() {
        let actual = toml::from_str::<Provider>(
            r#"
            transport = "rpc"
            url = "http://localhost:8545"
            label = "peering"
        "#,
        );

        assert_eq!(true, actual.is_err());
        let err_str = actual.unwrap_err().to_string();
        assert_eq!(
            err_str.contains("missing field `features`"),
            true,
            "{}",
            err_str
        );
    }

    #[test]
    fn fails_if_non_substreams_provider_for_substreams_protocol() {
        let mut actual = toml::from_str::<ChainSection>(
            r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "primary"
            protocol = "substreams"
            provider = [
              { label = "firehose", details = { type = "firehose", url = "http://127.0.0.1:8888", token = "TOKEN", features = ["filters"] }},
            ]
        "#,
        )
        .unwrap();
        let err = actual.validate().unwrap_err().to_string();

        assert!(err.contains("only supports substreams providers"), "{err}");
    }

    #[test]
    fn fails_if_only_substreams_provider_for_non_substreams_protocol() {
        let mut actual = toml::from_str::<ChainSection>(
            r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "primary"
            protocol = "ethereum"
            provider = [
              { label = "firehose", details = { type = "substreams", url = "http://127.0.0.1:8888", token = "TOKEN", features = ["filters"] }},
            ]
        "#,
        )
        .unwrap();
        let err = actual.validate().unwrap_err().to_string();

        assert!(
            err.contains("ethereum protocol requires an rpc or firehose endpoint defined"),
            "{err}"
        );
    }

    #[test]
    fn it_works_on_new_web3_provider_from_toml() {
        let actual = toml::from_str(
            r#"
            label = "peering"
            details = { type = "web3", transport = "ipc", url = "http://localhost:8545", features = ["archive"], headers = { x-test = "value" } }
        "#,
        )
        .unwrap();

        let mut features = BTreeSet::new();
        features.insert("archive".to_string());

        let mut headers = HeaderMap::new();
        headers.insert("x-test", HeaderValue::from_static("value"));

        assert_eq!(
            Provider {
                label: "peering".to_owned(),
                details: ProviderDetails::Web3(Web3Provider {
                    transport: Transport::Ipc,
                    url: "http://localhost:8545".to_owned(),
                    features,
                    headers,
                    rules: Vec::new(),
                }),
            },
            actual
        );
    }

    #[test]
    fn it_works_on_new_web3_provider_without_transport_from_toml() {
        let actual = toml::from_str(
            r#"
            label = "peering"
            details = { type = "web3", url = "http://localhost:8545", features = [] }
        "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "peering".to_owned(),
                details: ProviderDetails::Web3(Web3Provider {
                    transport: Transport::Rpc,
                    url: "http://localhost:8545".to_owned(),
                    features: BTreeSet::new(),
                    headers: HeaderMap::new(),
                    rules: Vec::new(),
                }),
            },
            actual
        );
    }

    #[test]
    fn it_errors_on_new_provider_with_deprecated_fields_from_toml() {
        let actual = toml::from_str::<Provider>(
            r#"
            label = "peering"
            url = "http://localhost:8545"
            details = { type = "web3", url = "http://localhost:8545", features = [] }
        "#,
        );

        assert_eq!(true, actual.is_err());
        let err_str = actual.unwrap_err().to_string();
        assert_eq!(err_str.contains("when `details` field is provided, deprecated `url`, `transport`, `features` and `headers` cannot be specified"),true, "{}", err_str);
    }

    #[test]
    fn it_works_on_new_firehose_provider_from_toml() {
        let actual = toml::from_str(
            r#"
                label = "firehose"
                details = { type = "firehose", url = "http://localhost:9000", features = [] }
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "firehose".to_owned(),
                details: ProviderDetails::Firehose(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![],
                }),
            },
            actual
        );
    }

    #[test]
    fn it_works_on_substreams_provider_from_toml() {
        let actual = toml::from_str(
            r#"
                label = "bananas"
                details = { type = "substreams", url = "http://localhost:9000", features = [] }
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "bananas".to_owned(),
                details: ProviderDetails::Substreams(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![],
                }),
            },
            actual
        );
    }
    #[test]
    fn it_works_on_new_firehose_provider_from_toml_no_features() {
        let mut actual = toml::from_str(
            r#"
                label = "firehose"
                details = { type = "firehose", url = "http://localhost:9000" }
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "firehose".to_owned(),
                details: ProviderDetails::Firehose(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![],
                }),
            },
            actual
        );
        assert! {actual.validate().is_ok()};
    }

    #[test]
    fn it_works_on_new_firehose_provider_with_doc_example_match() {
        let mut actual = toml::from_str(
            r#"
                label = "firehose"
                details = { type = "firehose", url = "http://localhost:9000" }
                match = [
                  { name = "some_node_.*", limit = 10 },
                  { name = "other_node_.*", limit = 0 } ]
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "firehose".to_owned(),
                details: ProviderDetails::Firehose(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![
                        Web3Rule {
                            name: Regex::new("some_node_.*").unwrap(),
                            limit: 10,
                        },
                        Web3Rule {
                            name: Regex::new("other_node_.*").unwrap(),
                            limit: 0,
                        }
                    ],
                }),
            },
            actual
        );
        assert! { actual.validate().is_ok()};
    }

    #[test]
    fn it_errors_on_firehose_provider_with_high_limit() {
        let mut actual = toml::from_str(
            r#"
                label = "substreams"
                details = { type = "substreams", url = "http://localhost:9000" }
                match = [
                  { name = "some_node_.*", limit = 101 },
                  { name = "other_node_.*", limit = 0 } ]
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "substreams".to_owned(),
                details: ProviderDetails::Substreams(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![
                        Web3Rule {
                            name: Regex::new("some_node_.*").unwrap(),
                            limit: 101,
                        },
                        Web3Rule {
                            name: Regex::new("other_node_.*").unwrap(),
                            limit: 0,
                        }
                    ],
                }),
            },
            actual
        );
        assert! { actual.validate().is_err()};
    }

    #[test]
    fn it_works_on_new_substreams_provider_with_doc_example_match() {
        let mut actual = toml::from_str(
            r#"
                label = "substreams"
                details = { type = "substreams", url = "http://localhost:9000" }
                match = [
                  { name = "some_node_.*", limit = 10 },
                  { name = "other_node_.*", limit = 0 } ]
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "substreams".to_owned(),
                details: ProviderDetails::Substreams(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![
                        Web3Rule {
                            name: Regex::new("some_node_.*").unwrap(),
                            limit: 10,
                        },
                        Web3Rule {
                            name: Regex::new("other_node_.*").unwrap(),
                            limit: 0,
                        }
                    ],
                }),
            },
            actual
        );
        assert! { actual.validate().is_ok()};
    }

    #[test]
    fn it_errors_on_substreams_provider_with_high_limit() {
        let mut actual = toml::from_str(
            r#"
                label = "substreams"
                details = { type = "substreams", url = "http://localhost:9000" }
                match = [
                  { name = "some_node_.*", limit = 101 },
                  { name = "other_node_.*", limit = 0 } ]
            "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "substreams".to_owned(),
                details: ProviderDetails::Substreams(FirehoseProvider {
                    url: "http://localhost:9000".to_owned(),
                    token: None,
                    features: BTreeSet::new(),
                    conn_pool_size: 20,
                    rules: vec![
                        Web3Rule {
                            name: Regex::new("some_node_.*").unwrap(),
                            limit: 101,
                        },
                        Web3Rule {
                            name: Regex::new("other_node_.*").unwrap(),
                            limit: 0,
                        }
                    ],
                }),
            },
            actual
        );
        assert! { actual.validate().is_err()};
    }

    #[test]
    fn it_works_on_new_firehose_provider_from_toml_unsupported_features() {
        let actual = toml::from_str::<Provider>(
            r#"
                label = "firehose"
                details = { type = "firehose", url = "http://localhost:9000", features = ["bananas"]}
            "#,
        ).unwrap().validate();
        assert_eq!(true, actual.is_err(), "{:?}", actual);

        if let Err(error) = actual {
            assert_eq!(
                true,
                error
                    .to_string()
                    .starts_with("supported firehose endpoint filters are:")
            )
        }
    }

    #[test]
    fn it_parses_web3_provider_rules() {
        fn limit_for(node: &str) -> SubgraphLimit {
            let prov = toml::from_str::<Web3Provider>(
                r#"
            label = "something"
            url = "http://example.com"
            features = []
            match = [ { name = "some_node_.*", limit = 10 },
                      { name = "other_node_.*", limit = 0 } ]
        "#,
            )
            .unwrap();

            prov.limit_for(&NodeId::new(node.to_string()).unwrap())
        }

        assert_eq!(SubgraphLimit::Limit(10), limit_for("some_node_0"));
        assert_eq!(SubgraphLimit::Disabled, limit_for("other_node_0"));
        assert_eq!(SubgraphLimit::Disabled, limit_for("default"));
    }

    #[test]
    fn it_parses_web3_default_empty_unlimited() {
        fn limit_for(node: &str) -> SubgraphLimit {
            let prov = toml::from_str::<Web3Provider>(
                r#"
            label = "something"
            url = "http://example.com"
            features = []
            match = []
        "#,
            )
            .unwrap();

            prov.limit_for(&NodeId::new(node.to_string()).unwrap())
        }

        assert_eq!(SubgraphLimit::Unlimited, limit_for("other_node_0"));
    }
    fn read_resource_as_string<P: AsRef<Path>>(path: P) -> String {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("resources/tests");
        d.push(path);

        read_to_string(&d).unwrap_or_else(|_| panic!("resource {:?} not found", &d))
    }

    #[test]
    fn it_works_on_web3call_provider_without_transport_from_toml() {
        let actual = toml::from_str(
            r#"
            label = "peering"
            details = { type = "web3call", url = "http://localhost:8545", features = [] }
        "#,
        )
        .unwrap();

        assert_eq!(
            Provider {
                label: "peering".to_owned(),
                details: ProviderDetails::Web3Call(Web3Provider {
                    transport: Transport::Rpc,
                    url: "http://localhost:8545".to_owned(),
                    features: BTreeSet::new(),
                    headers: HeaderMap::new(),
                    rules: Vec::new(),
                }),
            },
            actual
        );
    }

    #[test]
    fn web3rules_have_the_right_order() {
        assert!(SubgraphLimit::Unlimited > SubgraphLimit::Limit(10));
        assert!(SubgraphLimit::Limit(10) > SubgraphLimit::Disabled);
    }

    #[test]
    fn duplicated_labels_are_not_allowed_within_chain() {
        let mut actual = toml::from_str::<ChainSection>(
            r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "vip"
            provider = [
                { label = "mainnet1", url = "http://127.0.0.1", features = [], headers = { Authorization = "Bearer foo" } },
                { label = "mainnet1", url = "http://127.0.0.1", features = [ "archive", "traces" ] }
            ]
            "#,
        )
        .unwrap();

        let err = actual.validate();
        assert_eq!(true, err.is_err());
        let err = err.unwrap_err();
        assert_eq!(
            true,
            err.to_string().contains("unique"),
            "result: {:?}",
            err
        );
    }

    #[test]
    fn duplicated_labels_are_allowed_on_different_chain() {
        let mut actual = toml::from_str::<ChainSection>(
            r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "vip"
            provider = [
                { label = "mainnet1", url = "http://127.0.0.1", features = [], headers = { Authorization = "Bearer foo" } },
                { label = "mainnet2", url = "http://127.0.0.1", features = [ "archive", "traces" ] }
            ]
            [mainnet2]
            shard = "vip"
            provider = [
                { label = "mainnet1", url = "http://127.0.0.1", features = [], headers = { Authorization = "Bearer foo" } },
                { label = "mainnet2", url = "http://127.0.0.1", features = [ "archive", "traces" ] }
            ]
            "#,
        )
        .unwrap();

        let result = actual.validate();
        assert_eq!(true, result.is_ok(), "error: {:?}", result.unwrap_err());
    }

    #[test]
    fn polling_interval() {
        let default = default_polling_interval();
        let different = 2 * default;

        // Polling interval not set explicitly, use default
        let actual = toml::from_str::<ChainSection>(
            r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "vip"
            provider = []"#,
        )
        .unwrap();

        assert_eq!(
            default,
            actual.chains.get("mainnet").unwrap().polling_interval
        );

        // Polling interval set explicitly, use that
        let actual = toml::from_str::<ChainSection>(
            format!(
                r#"
            ingestor = "block_ingestor_node"
            [mainnet]
            shard = "vip"
            provider = []
            polling_interval = {}"#,
                different.as_millis()
            )
            .as_str(),
        )
        .unwrap();

        assert_eq!(
            different,
            actual.chains.get("mainnet").unwrap().polling_interval
        );
    }
}
