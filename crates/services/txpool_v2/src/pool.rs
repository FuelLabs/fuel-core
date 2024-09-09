use std::{
    collections::{
        BTreeMap,
        HashMap,
        HashSet,
        VecDeque,
    },
    time::{
        Duration,
        Instant,
    },
};

use fuel_core_types::{
    fuel_tx::{
        ContractId,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};
use num_rational::Ratio;
use petgraph::{
    graph::NodeIndex,
    prelude::StableGraph,
    visit::EdgeRef,
};
use tracing::instrument;

use crate::{
    config::Config,
    error::Error,
    registries::Registries,
};

struct GraphNode {
    transaction: PoolTransaction,
    /// The cumulative tip of a transaction and all of its children.
    cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    cumulative_gas: u64,
    /// Number of dependents
    number_dependents: u64,
}

pub type TxGraph = StableGraph<GraphNode, ()>;
pub type Parents = Vec<NodeIndex>;
pub type RatioTipGas = Ratio<u64>;

#[derive(Default)]
pub struct Collisions {
    pub reasons: HashSet<CollisionReason>,
    pub colliding_txs: Vec<NodeIndex>,
}

impl Collisions {
    pub fn new() -> Self {
        Default::default()
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum CollisionReason {
    Coin(UtxoId),
    Message(Nonce),
    ContractCreation(ContractId),
}

pub struct Pool {
    // TODO: Abstraction Storage trait
    graph: TxGraph,
    transactions_sorted_time: VecDeque<(NodeIndex, Instant)>,
    transactions_sorted_tip: BTreeMap<RatioTipGas, NodeIndex>,
    registries: Registries,
    config: Config,
}

impl Pool {
    pub fn new(config: Config) -> Self {
        Pool {
            graph: TxGraph::new(),
            registries: Default::default(),
            transactions_sorted_time: Default::default(),
            transactions_sorted_tip: Default::default(),
            config,
        }
    }

    #[instrument(skip(self))]
    pub fn insert(
        &mut self,
        transactions: Vec<PoolTransaction>,
    ) -> Vec<Result<(), Error>> {
        transactions
            .into_iter()
            .map(|tx| {
                let collisions = self.registries.gather_colliding_txs(&tx)?;
                if !self.is_better_than_collisions(&tx, collisions) {
                    return Err(Error::Collided("TODO".to_string()));
                }
                let parents = self.registries.check_and_gather_parent_txs(&tx)?;
                // No parents insert directly in the graph and the sorted transactions
                if parents.is_empty() {
                    let node = GraphNode {
                        cumulative_tip: tx.tip(),
                        cumulative_gas: tx.max_gas(),
                        transaction: tx,
                        number_dependents: 0,
                    };
                    let node_id = self.graph.add_node(node);
                    self.transactions_sorted_time
                        .push_back((node_id, Instant::now()));
                    self.transactions_sorted_tip
                        .insert(Ratio::new(tx.tip(), tx.max_gas()), node_id);
                    return Ok(());
                }
                // Remove collisions and their dependencies from the graph
                let mut to_remove = vec![];
                let mut to_check = collisions.colliding_txs;
                while to_check.len() > 0 {
                    let node_id = to_check.pop().unwrap();
                    to_remove.push(node_id);
                    for edge in self
                        .graph
                        .edges_directed(node_id, petgraph::Direction::Outgoing)
                    {
                        to_check.push(edge.target());
                    }
                }
                for node_id in to_remove {
                    self.graph.remove_node(node_id);
                }
                // Add the new transaction to the graph and update the others in consequence
                let tip = tx.tip();
                let gas = tx.max_gas();
                let node = GraphNode {
                    cumulative_tip: 0,
                    cumulative_gas: 0,
                    transaction: tx,
                    number_dependents: 0,
                };
                // Used to check if the chain dependency is too big and then update cumulative gas
                let mut whole_tx_chain = vec![];
                let mut to_check = parents;
                while to_check.len() > 0 {
                    let parent_node_id = to_check.pop().unwrap();
                    let parent_node = self.graph.node_weight_mut(parent_node_id).unwrap();
                    if parent_node.number_dependents >= self.config.max_txs_per_chain {
                        return Err(Error::NotInsertedChainDependencyTooBig);
                    }
                    whole_tx_chain.push(parent_node);
                    for edge in self
                        .graph
                        .edges_directed(parent_node_id, petgraph::Direction::Incoming)
                    {
                        to_check.push(edge.source());
                    }
                }
                let node_id = self.graph.add_node(node);
                whole_tx_chain.push(self.graph.node_weight_mut(node_id).unwrap());
                for parent in parents {
                    self.graph.add_edge(parent, node_id, ());
                }
                // Update the cumulative tip and gas of the parents and recursively their parents, etc.
                while whole_tx_chain.len() > 0 {
                    let node = whole_tx_chain.pop().unwrap();
                    node.cumulative_tip = node.cumulative_tip.saturating_add(tip);
                    node.cumulative_gas = node.cumulative_gas.saturating_add(gas);
                }
                self.transactions_sorted_time
                    .push_back((node_id, Instant::now()));
                Ok(())
            })
            .collect()
    }

    fn is_better_than_collisions(
        &mut self,
        tx: &PoolTransaction,
        collisions: Collisions,
    ) -> bool {
        let new_tx_ratio = Ratio::new(tx.tip(), tx.max_gas());
        let (total_tip, total_gas) = collisions.colliding_txs.iter().fold(
            (0u64, 0u64),
            |(total_tip, total_gas), node_id| {
                let dependent_tx = self
                    .graph
                    .node_weight(*node_id)
                    .expect("Transaction always should exist in `tx_info`");
                let total_tip = total_tip.saturating_add(dependent_tx.cumulative_tip);
                let total_gas = total_gas.saturating_add(dependent_tx.cumulative_gas);
                (total_tip, total_gas)
            },
        );

        let collision_tx_ratio = Ratio::new(total_tip, total_gas);

        new_tx_ratio > collision_tx_ratio
    }

    pub fn extract_transactions_for_block(
        &mut self,
    ) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }

    pub fn prune(&mut self) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }
}
