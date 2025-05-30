//! # Helpers for creating networks of nodes

use crate::{
    chain_config::{
        CoinConfig,
        coin_config_helpers::CoinConfigGenerator,
    },
    combined_database::CombinedDatabase,
    database::{
        Database,
        database_description::off_chain::OffChain,
    },
    fuel_core_graphql_api::storage::transactions::TransactionStatuses,
    p2p::Multiaddr,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_chain_config::{
    ConsensusConfig,
    StateConfig,
};
use fuel_core_p2p::{
    codecs::{
        gossipsub::GossipsubMessageHandler,
        request_response::RequestResponseMessageHandler,
    },
    network_service::FuelP2PService,
    p2p_service::FuelP2PEvent,
    request_response::messages::{
        RequestMessage,
        V2ResponseMessage,
    },
    service::to_message_acceptance,
};
use fuel_core_poa::{
    Trigger,
    ports::BlockImporter,
};
use fuel_core_storage::{
    StorageAsRef,
    transactional::AtomicView,
};
use fuel_core_types::{
    fuel_asm::{
        RegId,
        op,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        Transaction,
        TransactionBuilder,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        Address,
        Bytes32,
        ChainId,
    },
    secrecy::Secret,
    services::p2p::GossipsubMessageAcceptance,
    signer::SignMode,
};
use futures::StreamExt;
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use std::{
    collections::HashMap,
    ops::{
        Index,
        IndexMut,
    },
    time::Duration,
};
use tokio::sync::broadcast;

#[derive(Copy, Clone)]
pub enum BootstrapType {
    BootstrapNodes,
    ReservedNodes,
}

/// Set of values that will be overridden in the node's configuration
#[derive(Clone, Debug)]
pub struct CustomizeConfig {
    min_exec_gas_price: Option<u64>,
    max_functional_peers_connected: Option<u32>,
    max_discovery_peers_connected: Option<u32>,
}

impl CustomizeConfig {
    pub fn no_overrides() -> Self {
        Self {
            min_exec_gas_price: None,
            max_functional_peers_connected: None,
            max_discovery_peers_connected: None,
        }
    }

    pub fn min_gas_price(mut self, min_gas_price: u64) -> Self {
        self.min_exec_gas_price = Some(min_gas_price);
        self
    }

    pub fn max_functional_peers_connected(mut self, max_peers_connected: u32) -> Self {
        self.max_functional_peers_connected = Some(max_peers_connected);
        self
    }

    pub fn max_discovery_peers_connected(mut self, max_peers_connected: u32) -> Self {
        self.max_discovery_peers_connected = Some(max_peers_connected);
        self
    }
}

#[derive(Clone)]
/// Setup for a producer node
pub struct ProducerSetup {
    /// Name of the producer.
    pub name: String,
    /// Secret key used to sign blocks.
    pub secret: SecretKey,
    /// Number of test transactions to create for this producer.
    pub num_test_txs: usize,
    /// Enable full utxo stateful validation.
    pub utxo_validation: bool,
    /// Indicates the type of initial connections.
    pub bootstrap_type: BootstrapType,
    /// Config Overrides
    pub config_overrides: CustomizeConfig,
}

#[derive(Clone)]
/// Setup for a validator node
pub struct ValidatorSetup {
    /// Name of the validator.
    pub name: String,
    /// Public key of the producer to sync from.
    pub pub_key: Address,
    /// Enable full utxo stateful validation.
    pub utxo_validation: bool,
    /// Indicates the type of initial connections.
    pub bootstrap_type: BootstrapType,
    /// Config Overrides
    pub config_overrides: CustomizeConfig,
}

#[derive(Clone)]
pub struct BootstrapSetup {
    pub name: String,
    pub pub_key: Address,
}

pub struct Node {
    pub node: FuelService,
    pub db: Database,
    pub config: Config,
    pub test_txs: Vec<Transaction>,
}

pub struct Bootstrap {
    listeners: Vec<Multiaddr>,
    kill: broadcast::Sender<()>,
}

pub struct Nodes {
    pub bootstrap_nodes: Vec<Bootstrap>,
    pub producers: Vec<Node>,
    pub validators: Vec<Node>,
}

/// Nodes accessible by their name.
pub struct NamedNodes(pub HashMap<String, Node>);

impl Bootstrap {
    /// Spawn a bootstrap node.
    pub async fn new(node_config: &Config) -> anyhow::Result<Self> {
        let bootstrap_config = extract_p2p_config(node_config).await;
        let request_response_codec =
            RequestResponseMessageHandler::new(bootstrap_config.max_block_size);
        let gossipsub_codec = GossipsubMessageHandler::new();
        let (sender, _) =
            broadcast::channel(bootstrap_config.reserved_nodes.len().saturating_add(1));
        let mut bootstrap = FuelP2PService::new(
            sender,
            bootstrap_config,
            gossipsub_codec,
            request_response_codec,
        )
        .await?;
        bootstrap.start().await?;

        let listeners = bootstrap.multiaddrs();
        let (kill, mut shutdown) = broadcast::channel(1);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = shutdown.recv() => {
                        assert!(result.is_ok());
                        break;
                    }
                    event = bootstrap.next_event() => {
                        // The bootstrap node only forwards data without validating it.
                        match event {
                            Some(FuelP2PEvent::GossipsubMessage {
                                peer_id,
                                message_id,
                                ..
                            }) => {
                                bootstrap.report_message_validation_result(
                                    &message_id,
                                    peer_id,
                                    to_message_acceptance(&GossipsubMessageAcceptance::Accept)
                                )
                            },
                            Some(FuelP2PEvent::InboundRequestMessage {
                                request_id,
                                request_message
                            }) => {
                                if request_message == RequestMessage::TxPoolAllTransactionsIds {
                                    let _ = bootstrap.send_response_msg(
                                        request_id,
                                        V2ResponseMessage::TxPoolAllTransactionsIds(Ok(vec![])),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Ok(Bootstrap { listeners, kill })
    }

    pub fn listeners(&self) -> Vec<Multiaddr> {
        self.listeners.clone()
    }

    pub fn shutdown(&mut self) {
        self.kill.send(()).unwrap();
    }
}

// set of nodes with the given setups.
#[allow(clippy::arithmetic_side_effects)]
pub async fn make_nodes(
    bootstrap_setup: impl IntoIterator<Item = Option<BootstrapSetup>>,
    producers_setup: impl IntoIterator<Item = Option<ProducerSetup>>,
    validators_setup: impl IntoIterator<Item = Option<ValidatorSetup>>,
    config: Option<Config>,
) -> Nodes {
    let producers: Vec<_> = producers_setup.into_iter().collect();

    let mut rng = StdRng::seed_from_u64(11);

    let mut coin_generator = CoinConfigGenerator::new();
    let txs_coins: Vec<_> = producers
        .iter()
        .map(|p| {
            let num_test_txs = p.as_ref()?.num_test_txs;
            let all: Vec<_> = (0..num_test_txs)
                .map(|_| {
                    let secret = SecretKey::random(&mut rng);
                    let mut initial_coin = CoinConfig {
                        ..coin_generator.generate_with(secret, 10000)
                    };
                    // Shift idx to prevent overlapping utxo_ids when
                    // merging with existing coins from config
                    initial_coin.output_index += 100;
                    let tx = TransactionBuilder::script(
                        vec![op::ret(RegId::ONE)].into_iter().collect(),
                        vec![],
                    )
                    .script_gas_limit(100000)
                    .add_unsigned_coin_input(
                        secret,
                        initial_coin.utxo_id(),
                        initial_coin.amount,
                        initial_coin.asset_id,
                        Default::default(),
                    )
                    .finalize_as_transaction();

                    (tx, initial_coin)
                })
                .collect();
            Some(all)
        })
        .collect();

    let mut producers_with_txs = Vec::with_capacity(producers.len());

    let mut config = config.unwrap_or_else(Config::local_node);
    let mut state_config = StateConfig::from_reader(&config.snapshot_reader).unwrap();

    for (all, producer) in txs_coins.into_iter().zip(producers.into_iter()) {
        match all {
            Some(all) => {
                let mut txs = Vec::with_capacity(all.len());
                for (tx, initial_coin) in all {
                    txs.push(tx);
                    state_config.coins.push(initial_coin);
                }
                producers_with_txs.push(Some((producer.unwrap(), txs)));
            }
            None => {
                producers_with_txs.push(None);
            }
        }
    }

    config.snapshot_reader = config
        .snapshot_reader
        .clone()
        .with_state_config(state_config);

    let bootstrap_nodes: Vec<Bootstrap> =
        futures::stream::iter(bootstrap_setup.into_iter().enumerate())
            .then(|(i, boot)| {
                let config = config.clone();
                async move {
                    let config = config.clone();
                    let name = boot.as_ref().map_or(String::new(), |s| s.name.clone());
                    let mut node_config = make_config(
                        (!name.is_empty())
                            .then_some(name)
                            .unwrap_or_else(|| format!("b:{i}")),
                        config.clone(),
                        CustomizeConfig::no_overrides(),
                    );
                    if let Some(BootstrapSetup { pub_key, .. }) = boot {
                        update_signing_key(&mut node_config, pub_key);
                    }
                    Bootstrap::new(&node_config)
                        .await
                        .expect("Failed to create bootstrap node")
                }
            })
            .collect()
            .await;

    let boots: Vec<_> = bootstrap_nodes.iter().flat_map(|b| b.listeners()).collect();

    let mut producers = Vec::with_capacity(producers_with_txs.len());
    for (i, s) in producers_with_txs.into_iter().enumerate() {
        let config = config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.0.name.clone());
        let overrides = s
            .clone()
            .map_or(CustomizeConfig::no_overrides(), |s| s.0.config_overrides);
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("p:{i}")),
            config.clone(),
            overrides,
        );

        let mut test_txs = Vec::with_capacity(0);

        if let Some((
            ProducerSetup {
                secret,
                utxo_validation,
                bootstrap_type,
                ..
            },
            txs,
        )) = s
        {
            match bootstrap_type {
                BootstrapType::BootstrapNodes => {
                    node_config
                        .p2p
                        .as_mut()
                        .unwrap()
                        .bootstrap_nodes
                        .clone_from(&boots);
                }
                BootstrapType::ReservedNodes => {
                    node_config
                        .p2p
                        .as_mut()
                        .unwrap()
                        .reserved_nodes
                        .clone_from(&boots);
                }
            }

            node_config.utxo_validation = utxo_validation;
            node_config.txpool.utxo_validation = utxo_validation;
            update_signing_key(&mut node_config, Input::owner(&secret.public_key()));

            node_config.consensus_signer = SignMode::Key(Secret::new(secret.into()));

            if !txs.is_empty() {
                node_config
                    .pre_confirmation_signature_service
                    .echo_delegation_interval = Duration::from_millis(200);
            }

            test_txs = txs;
        }

        let producer = make_node(node_config, test_txs).await;
        producers.push(producer);
    }

    let mut validators = vec![];
    for (i, s) in validators_setup.into_iter().enumerate() {
        let config = config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.name.clone());
        let overrides = s
            .clone()
            .map_or(CustomizeConfig::no_overrides(), |s| s.config_overrides);
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("v:{i}")),
            config.clone(),
            overrides,
        );
        node_config.block_production = Trigger::Never;
        node_config.consensus_signer = SignMode::Unavailable;

        if let Some(ValidatorSetup {
            pub_key,
            utxo_validation,
            bootstrap_type,
            ..
        }) = s
        {
            node_config.utxo_validation = utxo_validation;

            match bootstrap_type {
                BootstrapType::BootstrapNodes => {
                    node_config
                        .p2p
                        .as_mut()
                        .unwrap()
                        .bootstrap_nodes
                        .clone_from(&boots);
                }
                BootstrapType::ReservedNodes => {
                    node_config
                        .p2p
                        .as_mut()
                        .unwrap()
                        .reserved_nodes
                        .clone_from(&boots);
                }
            }
            update_signing_key(&mut node_config, pub_key);
        }
        validators.push(make_node(node_config, Vec::with_capacity(0)).await)
    }

    Nodes {
        bootstrap_nodes,
        producers,
        validators,
    }
}

fn update_signing_key(config: &mut Config, key: Address) {
    let snapshot_reader = &config.snapshot_reader;

    let mut chain_config = snapshot_reader.chain_config().clone();
    match &mut chain_config.consensus {
        ConsensusConfig::PoA { signing_key } => {
            *signing_key = key;
        }
        ConsensusConfig::PoAV2(poa) => {
            poa.set_genesis_signing_key(key);
        }
    }
    config.snapshot_reader = snapshot_reader.clone().with_chain_config(chain_config)
}

pub fn make_config(
    name: String,
    mut node_config: Config,
    config_overrides: CustomizeConfig,
) -> Config {
    node_config.p2p = Config::local_node().p2p;
    node_config.utxo_validation = true;
    node_config.name = name;
    if let Some(min_gas_price) = config_overrides.min_exec_gas_price {
        node_config.gas_price_config.min_exec_gas_price = min_gas_price;
    }
    if let Some(p2p) = &mut node_config.p2p {
        if let Some(max_discovery_peers_connected) =
            config_overrides.max_discovery_peers_connected
        {
            p2p.max_discovery_peers_connected = max_discovery_peers_connected;
        }

        if let Some(max_functional_peers_connected) =
            config_overrides.max_functional_peers_connected
        {
            p2p.max_functional_peers_connected = max_functional_peers_connected;
        }
    }
    node_config
}

pub async fn make_node(node_config: Config, test_txs: Vec<Transaction>) -> Node {
    #[cfg(feature = "default")]
    let db = CombinedDatabase::from_config(&node_config.combined_db_config)
        .unwrap()
        .on_chain()
        .clone();
    #[cfg(not(feature = "default"))]
    let db = Database::in_memory();

    // Test coverage slows down the execution a lot, and while running all tests,
    // it may require a lot of time to start the node. We have a
    // timeout here to watch infinity loops, so it is okay to use 120 seconds.
    let time_limit = Duration::from_secs(120);
    let node = tokio::time::timeout(
        time_limit,
        FuelService::from_database(db.clone(), node_config),
    )
    .await
    .unwrap_or_else(|_| {
        panic!(
            "All services should start in less than {} seconds",
            time_limit.as_secs()
        )
    })
    .expect("The `FuelService should start without error");

    let config = node.shared.config.clone();
    Node {
        node,
        db,
        config,
        test_txs,
    }
}

async fn extract_p2p_config(node_config: &Config) -> fuel_core_p2p::config::Config {
    let bootstrap_config = node_config.p2p.clone();
    let db = CombinedDatabase::in_memory();
    crate::service::genesis::execute_and_commit_genesis_block(node_config, &db)
        .await
        .unwrap();
    bootstrap_config
        .unwrap()
        .init(db.on_chain().latest_view().unwrap().get_genesis().unwrap())
        .unwrap()
}

impl Node {
    /// Returns the vector of valid transactions for pre-initialized state.
    pub fn test_transactions(&self) -> &Vec<Transaction> {
        &self.test_txs
    }

    /// Waits for `number_of_blocks` and each block should be `is_local`
    pub async fn wait_for_blocks(&self, number_of_blocks: usize, is_local: bool) {
        let mut stream = self
            .node
            .shared
            .block_importer
            .block_stream()
            .take(number_of_blocks);
        while let Some(block) = stream.next().await {
            if block.is_locally_produced() != is_local {
                panic!(
                    "Block produced by the wrong node while was \
                    waiting for `{number_of_blocks}` and is_local=`{is_local}`"
                );
            }
        }
    }

    /// Wait for the node to reach consistency with the given transactions.
    pub async fn consistency(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        let db = self.node.shared.database.off_chain();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

        // first tick completes immediately
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let not_found = not_found_txs(db, txs);

                    if not_found.is_empty() {
                        break;
                    }
                }
                _ = self.node.await_shutdown() => {
                    panic!("Got a stop signal")
                }
            }
        }
    }

    pub async fn consistency_with_duration(
        &mut self,
        txs: &HashMap<Bytes32, Transaction>,
        duration: Duration,
    ) {
        tokio::time::timeout(duration, self.consistency(txs))
            .await
            .unwrap_or_else(|_| {
                panic!("Failed to reach consistency for {:?}", self.config.name)
            });
    }

    /// Wait for the node to reach consistency with the given transactions within 10 seconds.
    pub async fn consistency_10s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        self.consistency_with_duration(txs, Duration::from_secs(10))
            .await;
    }

    /// Wait for the node to reach consistency with the given transactions within 20 seconds.
    pub async fn consistency_20s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        self.consistency_with_duration(txs, Duration::from_secs(20))
            .await;
    }

    /// Wait for the node to reach consistency with the given transactions within 30 seconds.
    pub async fn consistency_30s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        self.consistency_with_duration(txs, Duration::from_secs(30))
            .await;
    }

    /// Insert the test transactions into the node's transaction pool.
    pub async fn insert_txs(&self) -> HashMap<Bytes32, Transaction> {
        let mut expected = HashMap::new();
        for tx in &self.test_txs {
            let tx_id = tx.id(&ChainId::default());
            self.node
                .shared
                .txpool_shared_state
                .insert(tx.clone())
                .await
                .unwrap();

            expected.insert(tx_id, tx.clone());
        }
        expected
    }

    /// Start a node that has been shutdown.
    /// Note that nodes always start running.
    pub async fn start(&mut self) {
        let node = FuelService::from_database(self.db.clone(), self.config.clone())
            .await
            .unwrap();
        self.node = node;
    }

    /// Stop a node.
    pub async fn shutdown(&mut self) {
        self.node
            .send_stop_signal_and_await_shutdown()
            .await
            .unwrap();
    }
}

fn not_found_txs<'iter>(
    db: &'iter Database<OffChain>,
    txs: &'iter HashMap<Bytes32, Transaction>,
) -> Vec<TxId> {
    let mut not_found = vec![];
    txs.iter().for_each(|(id, tx)| {
        assert_eq!(id, &tx.id(&Default::default()));
        let found = db
            .storage::<TransactionStatuses>()
            .contains_key(id)
            .unwrap();
        if !found {
            not_found.push(*id);
        }
    });
    not_found
}

impl ProducerSetup {
    pub fn new(secret: SecretKey) -> Self {
        Self::new_with_overrides(secret, CustomizeConfig::no_overrides())
    }

    pub fn new_with_overrides(
        secret: SecretKey,
        config_overrides: CustomizeConfig,
    ) -> Self {
        Self {
            name: Default::default(),
            secret,
            num_test_txs: Default::default(),
            utxo_validation: true,
            bootstrap_type: BootstrapType::BootstrapNodes,
            config_overrides,
        }
    }

    pub fn with_txs(self, num_test_txs: usize) -> Self {
        Self {
            num_test_txs,
            ..self
        }
    }

    pub fn with_name(self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..self
        }
    }

    pub fn utxo_validation(self, utxo_validation: bool) -> Self {
        Self {
            utxo_validation,
            ..self
        }
    }

    pub fn bootstrap_type(self, bootstrap_type: BootstrapType) -> Self {
        Self {
            bootstrap_type,
            ..self
        }
    }
}

impl ValidatorSetup {
    pub fn new(pub_key: Address) -> Self {
        Self::new_with_overrides(pub_key, CustomizeConfig::no_overrides())
    }

    pub fn new_with_overrides(
        pub_key: Address,
        config_overrides: CustomizeConfig,
    ) -> Self {
        Self {
            pub_key,
            name: Default::default(),
            utxo_validation: true,
            bootstrap_type: BootstrapType::BootstrapNodes,
            config_overrides,
        }
    }

    pub fn with_name(self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..self
        }
    }

    pub fn utxo_validation(self, utxo_validation: bool) -> Self {
        Self {
            utxo_validation,
            ..self
        }
    }

    pub fn bootstrap_type(self, bootstrap_type: BootstrapType) -> Self {
        Self {
            bootstrap_type,
            ..self
        }
    }
}
impl BootstrapSetup {
    pub fn new(pub_key: Address) -> Self {
        Self {
            pub_key,
            name: Default::default(),
        }
    }
}

impl From<Vec<Node>> for NamedNodes {
    fn from(nodes: Vec<Node>) -> Self {
        let nodes = nodes
            .into_iter()
            .map(|v| (v.config.name.clone(), v))
            .collect();
        Self(nodes)
    }
}

impl Index<&str> for NamedNodes {
    type Output = Node;

    fn index(&self, index: &str) -> &Self::Output {
        self.0.get(index).unwrap()
    }
}

impl IndexMut<&str> for NamedNodes {
    fn index_mut(&mut self, index: &str) -> &mut Self::Output {
        self.0.get_mut(index).unwrap()
    }
}

impl Drop for Bootstrap {
    fn drop(&mut self) {
        self.shutdown();
    }
}
