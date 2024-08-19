//! # Helpers for creating networks of nodes

use crate::{
    chain_config::{
        CoinConfig,
        CoinConfigGenerator,
    },
    combined_database::CombinedDatabase,
    database::Database,
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
    codecs::postcard::PostcardCodec,
    network_service::FuelP2PService,
    p2p_service::FuelP2PEvent,
    service::to_message_acceptance,
};
use fuel_core_poa::{
    ports::BlockImporter,
    signer::SignMode,
    Trigger,
};
use fuel_core_storage::{
    tables::Transactions,
    transactional::AtomicView,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
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
};
use futures::StreamExt;
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    collections::HashMap,
    ops::{
        Index,
        IndexMut,
    },
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;

#[derive(Copy, Clone)]
pub enum BootstrapType {
    BootstrapNodes,
    ReservedNodes,
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
    pub async fn new(node_config: &Config) -> Self {
        let bootstrap_config = extract_p2p_config(node_config).await;
        let codec = PostcardCodec::new(bootstrap_config.max_block_size);
        let (sender, _) =
            broadcast::channel(bootstrap_config.reserved_nodes.len().saturating_add(1));
        let mut bootstrap = FuelP2PService::new(sender, bootstrap_config, codec);
        bootstrap.start().await.unwrap();

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
                        if let Some(FuelP2PEvent::GossipsubMessage {
                            peer_id,
                            message_id,
                            ..
                        }) = event {
                            bootstrap.report_message_validation_result(
                                &message_id,
                                peer_id,
                                to_message_acceptance(&GossipsubMessageAcceptance::Accept)
                            )
                        }
                    }
                }
            }
        });

        Bootstrap { listeners, kill }
    }

    pub fn listeners(&self) -> Vec<Multiaddr> {
        self.listeners.clone()
    }

    pub fn shutdown(&mut self) {
        self.kill.send(()).unwrap();
    }
}

// set of nodes with the given setups.
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
                    let initial_coin = CoinConfig {
                        // set idx to prevent overlapping utxo_ids when
                        // merging with existing coins from config
                        output_index: 2,
                        ..coin_generator.generate_with(secret, 10000)
                    };
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
                    );
                    if let Some(BootstrapSetup { pub_key, .. }) = boot {
                        update_signing_key(&mut node_config, pub_key);
                    }
                    Bootstrap::new(&node_config).await
                }
            })
            .collect()
            .await;

    let boots: Vec<_> = bootstrap_nodes.iter().flat_map(|b| b.listeners()).collect();

    let mut producers = Vec::with_capacity(producers_with_txs.len());
    for (i, s) in producers_with_txs.into_iter().enumerate() {
        let config = config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.0.name.clone());
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("p:{i}")),
            config.clone(),
        );

        let mut test_txs = Vec::with_capacity(0);
        node_config.block_production = Trigger::Instant;

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
            update_signing_key(&mut node_config, Input::owner(&secret.public_key()));

            node_config.consensus_signer = SignMode::Key(Secret::new(secret.into()));

            test_txs = txs;
        }

        let producer = make_node(node_config, test_txs).await;
        producers.push(producer);
    }

    let mut validators = vec![];
    for (i, s) in validators_setup.into_iter().enumerate() {
        let config = config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.name.clone());
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("v:{i}")),
            config.clone(),
        );
        node_config.block_production = Trigger::Never;

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

pub fn make_config(name: String, mut node_config: Config) -> Config {
    node_config.p2p = Config::local_node().p2p;
    node_config.utxo_validation = true;
    node_config.name = name;
    node_config
}

pub async fn make_node(node_config: Config, test_txs: Vec<Transaction>) -> Node {
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
        let Self { db, .. } = self;
        let mut blocks = self.node.shared.block_importer.block_stream();
        while !not_found_txs(db, txs).is_empty() {
            tokio::select! {
                result = blocks.next() => {
                    result.unwrap();
                }
                _ = self.node.await_shutdown() => {
                    panic!("Got a stop signal")
                }
            }
        }

        let count = db
            .all_transactions(None, None)
            .filter_ok(|tx| tx.is_script())
            .count();
        assert_eq!(count, txs.len());
    }

    /// Wait for the node to reach consistency with the given transactions within 10 seconds.
    pub async fn consistency_10s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        tokio::time::timeout(Duration::from_secs(10), self.consistency(txs))
            .await
            .unwrap_or_else(|_| {
                panic!("Failed to reach consistency for {:?}", self.config.name)
            });
    }

    /// Wait for the node to reach consistency with the given transactions within 20 seconds.
    pub async fn consistency_20s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        tokio::time::timeout(Duration::from_secs(20), self.consistency(txs))
            .await
            .unwrap_or_else(|_| {
                panic!("Failed to reach consistency for {:?}", self.config.name)
            });
    }

    /// Insert the test transactions into the node's transaction pool.
    pub async fn insert_txs(&self) -> HashMap<Bytes32, Transaction> {
        let mut expected = HashMap::new();
        for tx in &self.test_txs {
            let tx_result = self
                .node
                .shared
                .txpool_shared_state
                .insert(vec![Arc::new(tx.clone())])
                .await
                .pop()
                .unwrap()
                .unwrap();

            let tx = Transaction::from(tx_result.inserted.as_ref());
            expected.insert(tx.id(&ChainId::default()), tx);

            assert!(tx_result.removed.is_empty());
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
    db: &'iter Database,
    txs: &'iter HashMap<Bytes32, Transaction>,
) -> Vec<TxId> {
    let mut not_found = vec![];
    txs.iter().for_each(|(id, tx)| {
        assert_eq!(id, &tx.id(&Default::default()));
        if !db.storage::<Transactions>().contains_key(id).unwrap() {
            not_found.push(*id);
        }
    });
    not_found
}

impl ProducerSetup {
    pub fn new(secret: SecretKey) -> Self {
        Self {
            name: Default::default(),
            secret,
            num_test_txs: Default::default(),
            utxo_validation: true,
            bootstrap_type: BootstrapType::BootstrapNodes,
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
        Self {
            pub_key,
            name: Default::default(),
            utxo_validation: true,
            bootstrap_type: BootstrapType::BootstrapNodes,
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
