//! # Helpers for creating networks of nodes

use fuel_core::{
    chain_config::ChainConfig,
    database::Database,
    p2p::Multiaddr,
    service::{
        genesis::maybe_initialize_state,
        Config,
        FuelService,
        ServiceTrait,
    },
};
use fuel_core_p2p::{
    codecs::postcard::PostcardCodec,
    network_service::FuelP2PService,
};
use fuel_core_poa::Trigger;
use fuel_core_storage::{
    tables::Transactions,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_asm::Opcode,
    fuel_crypto::{
        PublicKey,
        SecretKey,
    },
    fuel_tx::{
        Input,
        Transaction,
        TransactionBuilder,
        TxId,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::{
        Address,
        Bytes32,
    },
    fuel_vm::consts::REG_ONE,
    secrecy::Secret,
    services::block_importer::ImportResult,
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
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

#[derive(Clone)]
/// Setup for a producer node
pub struct ProducerSetup {
    /// Name of the producer.
    pub name: String,
    /// Secret key used to sign blocks.
    pub secret: SecretKey,
    /// Number of test transactions to create for this producer.
    pub num_test_txs: usize,
}

#[derive(Clone)]
/// Setup for a validator node
pub struct ValidatorSetup {
    /// Name of the validator.
    pub name: String,
    /// Public key of the producer to sync from.
    pub pub_key: Address,
}

pub struct Node {
    pub node: FuelService,
    pub db: Database,
    pub config: Config,
    pub test_txs: Vec<Transaction>,
    pub block_subscription: tokio::sync::broadcast::Receiver<Arc<ImportResult>>,
}

pub struct Nodes {
    pub bootstrap_nodes: Vec<Vec<Multiaddr>>,
    pub producers: Vec<Node>,
    pub validators: Vec<Node>,
}

/// Nodes accessible by their name.
pub struct NamedNodes(pub HashMap<String, Node>);

/// Spawn a bootstrap node.
async fn bootstrap_node(node_config: &Config, pub_key: PublicKey) -> Vec<Multiaddr> {
    let bootstrap_config = make_config(
        format!("{}:bootstrap", pub_key),
        node_config.chain_conf.clone(),
    )
    .p2p;
    let mut db = Database::in_memory();
    maybe_initialize_state(&node_config, &mut db).unwrap();
    let bootstrap_config = bootstrap_config.init(db.get_genesis().unwrap()).unwrap();
    let max_block_size = bootstrap_config.max_block_size;
    let mut bootstrap =
        FuelP2PService::new(bootstrap_config, PostcardCodec::new(max_block_size));
    bootstrap.start().unwrap();

    let multiaddrs;
    loop {
        let listeners = bootstrap.listeners().map(Clone::clone).collect::<Vec<_>>();
        if listeners.len() > 1 {
            multiaddrs = listeners;
            break
        }
        bootstrap.next_event().await;
    }
    let multiaddrs: Vec<Multiaddr> = multiaddrs
        .into_iter()
        .map(|addr| {
            format!("{}/p2p/{}", addr, &bootstrap.local_peer_id)
                .parse()
                .unwrap()
        })
        .collect();

    // TODO: Add shutdown
    tokio::spawn(async move {
        loop {
            bootstrap.next_event().await;
        }
    });

    multiaddrs
}

/// Create a set of nodes with the given setups.
pub async fn make_nodes(
    producers_setup: impl IntoIterator<Item = Option<ProducerSetup>>,
    validators_setup: impl IntoIterator<Item = Option<ValidatorSetup>>,
) -> Nodes {
    let producers: Vec<_> = producers_setup.into_iter().collect();

    let mut rng = StdRng::seed_from_u64(11);

    let txs_coins: Vec<_> = producers
        .iter()
        .map(|p| {
            let num_test_txs = p.as_ref()?.num_test_txs;
            let all: Vec<_> = (0..num_test_txs)
                .map(|_| {
                    let secret = SecretKey::random(&mut rng);
                    let utxo_id: UtxoId = rng.gen();
                    let initial_coin =
                        ChainConfig::initial_coin(secret, 10000, Some(utxo_id));
                    let tx = TransactionBuilder::script(
                        vec![Opcode::RET(REG_ONE)].into_iter().collect(),
                        vec![],
                    )
                    .gas_limit(100000)
                    .add_unsigned_coin_input(
                        secret,
                        utxo_id,
                        initial_coin.amount,
                        initial_coin.asset_id,
                        Default::default(),
                        0,
                    )
                    .finalize_as_transaction();

                    (tx, initial_coin)
                })
                .collect();
            Some(all)
        })
        .collect();

    let mut producers_with_txs = Vec::with_capacity(producers.len());
    let mut chain_config = ChainConfig::local_testnet();

    for (all, producer) in txs_coins.into_iter().zip(producers.into_iter()) {
        match all {
            Some(all) => {
                let mut txs = Vec::with_capacity(all.len());
                for (tx, initial_coin) in all {
                    txs.push(tx);
                    chain_config
                        .initial_state
                        .as_mut()
                        .unwrap()
                        .coins
                        .as_mut()
                        .unwrap()
                        .push(initial_coin);
                }
                producers_with_txs.push(Some((producer.unwrap(), txs)));
            }
            None => {
                producers_with_txs.push(None);
            }
        }
    }

    let mut producers = vec![];
    let mut bootstrap_nodes = vec![];
    for (i, s) in producers_with_txs.into_iter().enumerate() {
        let chain_config = chain_config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.0.name.clone());
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("p:{}", i)),
            chain_config.clone(),
        );

        let mut test_txs = Vec::with_capacity(0);
        node_config.block_production = Trigger::Instant;

        let mut pub_key = Default::default();
        if let Some((ProducerSetup { secret, .. }, txs)) = s {
            pub_key = secret.public_key();
            match &mut node_config.chain_conf.consensus {
                fuel_core::chain_config::ConsensusConfig::PoA { signing_key } => {
                    *signing_key = Input::owner(&pub_key);
                }
            }

            node_config.consensus_key = Some(Secret::new(secret.into()));

            test_txs = txs;
        }

        let multiaddrs = bootstrap_node(&node_config, pub_key).await;
        bootstrap_nodes.push(multiaddrs.clone());

        node_config.p2p.bootstrap_nodes = multiaddrs;
        let producer = make_node(node_config, test_txs).await;
        producers.push(producer);
    }

    let bootstrap_peers: Vec<_> = bootstrap_nodes
        .iter()
        .flat_map(|multiaddrs| multiaddrs.clone().into_iter())
        .collect();

    let mut validators = vec![];
    for (i, s) in validators_setup.into_iter().enumerate() {
        let chain_config = chain_config.clone();
        let name = s.as_ref().map_or(String::new(), |s| s.name.clone());
        let mut node_config = make_config(
            (!name.is_empty())
                .then_some(name)
                .unwrap_or_else(|| format!("v:{}", i)),
            chain_config.clone(),
        );
        node_config.p2p.bootstrap_nodes = bootstrap_peers.clone();

        if let Some(ValidatorSetup { pub_key, .. }) = s {
            match &mut node_config.chain_conf.consensus {
                fuel_core::chain_config::ConsensusConfig::PoA { signing_key } => {
                    *signing_key = pub_key;
                }
            }
        }
        validators.push(make_node(node_config, Vec::with_capacity(0)).await)
    }

    Nodes {
        bootstrap_nodes,
        producers,
        validators,
    }
}

fn make_config(name: String, chain_config: ChainConfig) -> Config {
    let mut node_config = Config::local_node();
    node_config.chain_conf = chain_config;
    node_config.utxo_validation = true;
    node_config.name = name;
    node_config
}

async fn make_node(node_config: Config, test_txs: Vec<Transaction>) -> Node {
    let db = Database::in_memory();
    let node = FuelService::from_database(db.clone(), node_config)
        .await
        .unwrap();

    let block_subscription = node.shared.block_importer.block_importer.subscribe();
    let config = node.shared.config.clone();
    Node {
        node,
        db,
        config,
        test_txs,
        block_subscription,
    }
}

impl Node {
    /// Wait for the node to reach consistency with the given transactions.
    pub async fn consistency(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        let Self {
            db,
            block_subscription,
            ..
        } = self;
        while !not_found_txs(db, txs).is_empty() {
            block_subscription.recv().await.unwrap();
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

    /// Wait for the node to reach consistency with the given transactions within 240 seconds.
    pub async fn consistency_240s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        tokio::time::timeout(Duration::from_secs(240), self.consistency(txs))
            .await
            .unwrap_or_else(|_| {
                panic!("Failed to reach consistency for {:?}", self.config.name)
            });
    }

    /// Insert the test transactions into the node's transaction pool.
    pub fn insert_txs(&self) -> HashMap<Bytes32, Transaction> {
        let mut expected = HashMap::new();
        for tx in &self.test_txs {
            let tx_result = self
                .node
                .shared
                .txpool
                .insert(vec![Arc::new(tx.clone())])
                .pop()
                .unwrap()
                .unwrap();

            let tx = Transaction::from(tx_result.inserted.as_ref());
            expected.insert(tx.id(), tx);

            assert!(tx_result.removed.is_empty());
        }
        expected
    }

    /// Start a node that has been shutdown.
    /// Note that nodes always start running.
    pub async fn start(&mut self) {
        let mut config =
            make_config(self.config.name.clone(), self.config.chain_conf.clone());
        config.p2p.bootstrap_nodes = self.config.p2p.bootstrap_nodes.clone();
        let node = FuelService::from_database(self.db.clone(), config.clone())
            .await
            .unwrap();
        self.node = node;
        self.config = config;
        self.block_subscription =
            self.node.shared.block_importer.block_importer.subscribe();
    }

    /// Stop a node.
    pub async fn shutdown(&mut self) {
        self.node.stop_and_await().await.unwrap();
    }
}

fn not_found_txs<'iter>(
    db: &'iter Database,
    txs: &'iter HashMap<Bytes32, Transaction>,
) -> Vec<TxId> {
    let mut not_found = vec![];
    txs.iter().for_each(|(id, tx)| {
        assert_eq!(id, &tx.id());
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
}

impl ValidatorSetup {
    pub fn new(pub_key: Address) -> Self {
        Self {
            pub_key,
            name: Default::default(),
        }
    }

    pub fn with_name(self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..self
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
