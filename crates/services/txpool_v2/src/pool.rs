use std::{
    collections::HashMap,
    time::Duration,
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
    graph::NodeIndex, prelude::StableGraph, visit::EdgeRef
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
}

pub type TxGraph = StableGraph<GraphNode, ()>;
pub type Parents = Vec<TxId>;
pub type Collisions = HashMap<CollisionReason, TxId>;

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum CollisionReason {
    Coin(UtxoId),
    Message(Nonce),
    ContractCreation(ContractId),
}

pub struct Pool {
    // TODO: Abstraction Storage trait
    graph: TxGraph,
    // TODO: Try to remove
    tx_id_to_node_id: HashMap<TxId, NodeIndex>,
    registries: Registries,
    config: Config,
}

impl Pool {
    pub fn new(config: Config) -> Self {
        Pool {
            graph: TxGraph::new(),
            tx_id_to_node_id: Default::default(),
            registries: Default::default(),
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
                // TODO: If no parents add the transaction to the graph and to the executables directly
                // Remove collisions and their dependencies from the graph
                let mut to_remove = vec![];
                let mut to_check = collisions
                    .iter()
                    .map(|(_, tx_id)| {
                        *self
                            .tx_id_to_node_id
                            .get(tx_id)
                            .expect("Transaction always should exist in `tx_info`")
                    })
                    .collect::<Vec<_>>();
                while to_check.len() > 0 {
                    let node_id = to_check.pop().unwrap();
                    to_remove.push(node_id);
                    for edge in self.graph.edges_directed(node_id, petgraph::Direction::Outgoing) {
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
                    cumulative_tip: tx.tip(),
                    cumulative_gas: tx.max_gas(),
                    transaction: tx,
                };
                let node_id = self.graph.add_node(node);
                self.tx_id_to_node_id.insert(tx.id(), node_id);
                for parent in parents {
                    let parent_node_id = *self
                        .tx_id_to_node_id
                        .get(&parent)
                        .expect("Parent always should exist in `tx_info`");
                    self.graph.add_edge(parent_node_id, node_id, ());
                }
                // Update the cumulative tip and gas of the parents and recursively their parents, etc.
                let mut to_update = vec![node_id];
                while to_update.len() > 0 {
                    let node_id = to_update.pop().unwrap();
                    let node = self.graph.node_weight_mut(node_id).unwrap();
                    for edge in self.graph.edges_directed(node_id, petgraph::Direction::Incoming) {
                        let parent_node_id = edge.source();
                        let parent_node = self.graph.node_weight(parent_node_id).unwrap();
                        parent_node.cumulative_tip = parent_node.cumulative_tip.saturating_add(tip);
                        parent_node.cumulative_gas = parent_node.cumulative_gas.saturating_add(gas);
                        to_update.push(parent_node_id);
                    }
                }
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
        let (total_tip, total_gas) =
            collisions
                .iter()
                .fold((0u64, 0u64), |(total_tip, total_gas), (_, tx_id)| {
                    let dependent_tx =
                        self.graph
                            .node_weight(
                                *self.tx_id_to_node_id.get(tx_id).expect(
                                    "Transaction always should exist in `tx_info`",
                                ),
                            )
                            .expect("Transaction always should exist in `tx_info`");
                    let total_tip = total_tip.saturating_add(dependent_tx.cumulative_tip);
                    let total_gas = total_gas.saturating_add(dependent_tx.cumulative_gas);
                    (total_tip, total_gas)
                });

        let collision_tx_ratio = Ratio::new(total_tip, total_gas);

        new_tx_ratio > collision_tx_ratio
    }

    pub fn extract_transactions_for_block(
        &mut self,
    ) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }

    pub fn prune(&mut self, tx_ttl: Duration) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }
}
