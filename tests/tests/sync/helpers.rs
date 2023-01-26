//! # Helpers for creating networks of nodes

use fuel_core::{
    chain_config::ChainConfig,
    database::Database,
    service::{
        Config,
        FuelService,
        ServiceTrait,
    },
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::Opcode,
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        Transaction,
        TransactionBuilder,
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
use futures::StreamExt;
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
    pub producers: Vec<Node>,
    pub validators: Vec<Node>,
}

/// Nodes accessible by their name.
pub struct NamedNodes(pub HashMap<String, Node>);

/// Create a set of nodes with the given setups.
pub async fn make_nodes(
    producers: impl IntoIterator<Item = Option<ProducerSetup>>,
    validators: impl IntoIterator<Item = Option<ValidatorSetup>>,
) -> Nodes {
    let producers: Vec<_> = producers.into_iter().collect();

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

    let producers: Vec<_> =
        futures::stream::iter(producers_with_txs.into_iter().enumerate())
            .then(|(i, s)| {
                let chain_config = chain_config.clone();
                async move {
                    let name = s.as_ref().map_or(String::new(), |s| s.0.name.clone());
                    let mut node_config = make_config(
                        (!name.is_empty())
                            .then_some(name)
                            .unwrap_or_else(|| format!("p:{}", i)),
                        chain_config.clone(),
                    );

                    let mut test_txs = Vec::with_capacity(0);
                    node_config.block_production = Trigger::Instant;

                    if let Some((ProducerSetup { secret, .. }, txs)) = s {
                        match &mut node_config.chain_conf.consensus {
                            fuel_core::chain_config::ConsensusConfig::PoA {
                                signing_key,
                            } => {
                                *signing_key = Input::owner(&secret.public_key());
                            }
                        }

                        node_config.consensus_key = Some(Secret::new(secret.into()));

                        test_txs = txs;
                    }

                    make_node(node_config, test_txs).await
                }
            })
            .collect()
            .await;

    let validators: Vec<_> = futures::stream::iter(validators.into_iter().enumerate())
        .then(|(i, s)| {
            let chain_config = chain_config.clone();
            async move {
                let name = s.as_ref().map_or(String::new(), |s| s.name.clone());
                let mut node_config = make_config(
                    (!name.is_empty())
                        .then_some(name)
                        .unwrap_or_else(|| format!("v:{}", i)),
                    chain_config.clone(),
                );

                if let Some(ValidatorSetup { pub_key, .. }) = s {
                    match &mut node_config.chain_conf.consensus {
                        fuel_core::chain_config::ConsensusConfig::PoA { signing_key } => {
                            *signing_key = pub_key;
                        }
                    }
                }
                make_node(node_config, Vec::with_capacity(0)).await
            }
        })
        .collect()
        .await;

    Nodes {
        producers,
        validators,
    }
}

fn make_config(name: String, chain_config: ChainConfig) -> Config {
    let mut node_config = Config::local_node();
    node_config.chain_conf = chain_config;
    node_config.utxo_validation = true;
    node_config.p2p.enable_mdns = true;
    node_config.name = name;
    node_config
}

async fn make_node(mut node_config: Config, test_txs: Vec<Transaction>) -> Node {
    let db = Database::in_memory();
    FuelService::make_config_consistent(&mut node_config);
    let node = FuelService::from_database(db.clone(), node_config.clone())
        .await
        .unwrap();

    let block_subscription = node.shared.block_importer.block_importer.subscribe();
    Node {
        node,
        db,
        config: node_config,
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
        while !has_txs(db, txs) {
            let _ = block_subscription.recv().await;
        }
    }

    /// Wait for the node to reach consistency with the given transactions within 10 seconds.
    pub async fn consistency_10s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        tokio::time::timeout(Duration::from_secs(10), self.consistency(txs))
            .await
            .expect("Failed to reach consistency");
    }

    /// Wait for the node to reach consistency with the given transactions within 20 seconds.
    pub async fn consistency_20s(&mut self, txs: &HashMap<Bytes32, Transaction>) {
        tokio::time::timeout(Duration::from_secs(20), self.consistency(txs))
            .await
            .expect("Failed to reach consistency");
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
        let node = FuelService::from_database(self.db.clone(), self.config.clone())
            .await
            .unwrap();
        self.node = node;
    }

    /// Stop a node.
    pub async fn shutdown(&mut self) {
        self.node.stop_and_await().await.unwrap();
    }
}

fn has_txs<'iter>(
    db: &'iter Database,
    txs: &'iter HashMap<Bytes32, Transaction>,
) -> bool {
    let mut empty = true;
    let r = db
        .all_transactions(None, None)
        .map(Result::unwrap)
        .filter(|tx| tx.is_script())
        .all(|tx| {
            empty = false;
            txs.contains_key(&tx.id())
        });
    r && !empty
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
