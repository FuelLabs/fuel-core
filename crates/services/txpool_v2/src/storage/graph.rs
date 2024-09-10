use std::collections::HashMap;

use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        BlobId,
        ContractId,
        Input,
        UtxoId,
    },
    services::txpool::PoolTransaction,
};
use petgraph::{
    graph::NodeIndex,
    prelude::StableDiGraph,
    visit::EdgeRef,
};

use crate::{
    collision_manager::CollisionReason,
    error::Error,
    ports::TxPoolDb,
};

use super::{
    Storage,
    StorageData,
};

pub struct GraphStorage {
    /// The configuration of the graph
    config: GraphConfig,
    /// The graph of transactions
    graph: StableDiGraph<StorageData, ()>,
    /// Coins -> Transaction that crurrently create the UTXO
    coins_creators: HashMap<UtxoId, NodeIndex>,
    /// Contract -> Transaction that currenty create the contract
    contracts_creators: HashMap<ContractId, NodeIndex>,
    /// Blob -> Transaction that currently create the blob
    blobs_creators: HashMap<BlobId, NodeIndex>,
}

pub struct GraphConfig {
    pub max_txs_per_chain: u64,
}

impl GraphStorage {
    pub fn new(config: GraphConfig) -> Self {
        Self {
            config,
            graph: StableDiGraph::new(),
            coins_creators: HashMap::new(),
            contracts_creators: HashMap::new(),
            blobs_creators: HashMap::new(),
        }
    }
}

impl GraphStorage {
    fn remove_node_and_dependent_sub_graph(&mut self, root: NodeIndex) {
        let mut to_traverse = vec![root];
        let mut to_remove = vec![];
        while let Some(node_id) = to_traverse.pop() {
            to_remove.push(node_id);
            for edge in self
                .graph
                .edges_directed(node_id, petgraph::Direction::Outgoing)
            {
                to_traverse.push(edge.target());
            }
        }

        let mut dependencies: Vec<NodeIndex> = self
            .graph
            .neighbors_directed(root, petgraph::Direction::Incoming)
            .collect();
        let nb_removed = to_remove.len() as u64;
        let (gas_removed, tip_removed) =
            to_remove.iter().fold((0, 0), |(gas, tip), node_id| {
                let node = self.graph.remove_node(*node_id).unwrap();
                (gas + node.cumulative_gas, tip + node.cumulative_tip)
            });

        while let Some(node_id) = dependencies.pop() {
            dependencies.extend(
                self.graph
                    .neighbors_directed(node_id, petgraph::Direction::Incoming),
            );
            let node = self.graph.node_weight_mut(node_id).unwrap();
            node.cumulative_gas = node.cumulative_gas.saturating_sub(gas_removed);
            node.cumulative_tip = node.cumulative_tip.saturating_sub(tip_removed);
            node.number_dependents -= nb_removed;
        }
    }
}

impl Storage for GraphStorage {
    type StorageIndex = NodeIndex;

    fn store_transaction(
        &mut self,
        transaction: PoolTransaction,
        dependencies: Vec<Self::StorageIndex>,
        collided_transactions: Vec<Self::StorageIndex>,
    ) -> Result<Self::StorageIndex, Error> {
        // No parents insert directly in the graph
        if dependencies.is_empty() {
            let node = StorageData {
                cumulative_tip: transaction.tip(),
                cumulative_gas: transaction.max_gas(),
                transaction,
                number_dependents: 0,
            };
            let node_id = self.graph.add_node(node);
            return Ok(node_id);
        }
        // Remove collisions and their dependencies from the graph
        for collision in collided_transactions {
            self.remove_node_and_dependent_sub_graph(collision);
        }
        // Add the new transaction to the graph and update the others in consequence
        let tip = transaction.tip();
        let gas = transaction.max_gas();
        let node = StorageData {
            cumulative_tip: tip,
            cumulative_gas: gas,
            transaction,
            number_dependents: 0,
        };

        let mut whole_tx_chain = vec![];

        // Check if the dependency chain is too big
        let mut to_check = dependencies.clone();
        while let Some(node_id) = to_check.pop() {
            let parent_node = self.graph.node_weight(node_id).unwrap();
            if parent_node.number_dependents >= self.config.max_txs_per_chain {
                return Err(Error::NotInsertedChainDependencyTooBig);
            }
            whole_tx_chain.push(node_id);
            for edge in self
                .graph
                .edges_directed(node_id, petgraph::Direction::Incoming)
            {
                to_check.push(edge.source());
            }
        }

        // Add the transaction to the graph
        let node_id = self.graph.add_node(node);
        whole_tx_chain.push(node_id);
        for parent in dependencies {
            self.graph.add_edge(parent, node_id, ());
        }

        // Update the cumulative tip and gas of the parents and recursively their parents, etc.
        for node_id in whole_tx_chain {
            let node = self.graph.node_weight_mut(node_id).unwrap();
            node.cumulative_tip = node.cumulative_tip.saturating_add(tip);
            node.cumulative_gas = node.cumulative_gas.saturating_add(gas);
        }
        Ok(node_id)
    }

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error> {
        self.graph
            .node_weight(*index)
            .ok_or(Error::TransactionNotFound(format!(
                "Transaction with index {:?} not found",
                index
            )))
    }

    fn get_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error> {
        Ok(self
            .graph
            .neighbors_directed(index, petgraph::Direction::Incoming)
            .collect())
    }

    fn get_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error> {
        Ok(self
            .graph
            .neighbors_directed(index, petgraph::Direction::Outgoing)
            .collect())
    }

    fn collect_dependencies_transactions(
        &self,
        transaction: &PoolTransaction,
        collisions: std::collections::HashSet<CollisionReason>,
        db: &impl TxPoolDb,
        utxo_validation: bool,
    ) -> Result<Vec<Self::StorageIndex>, Error> {
        // TODO: Finish the function
        let mut pool_parents = Vec::new();
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // If the utxo collides it means we already made the verifications
                    // If the utxo is created in the pool, need to check if we don't spend too much (utxo can still be unresolved)
                    // If the utxo_validation is active, we need to check if the utxo exists in the database and is valid
                    if collisions.contains(&CollisionReason::Coin(*utxo_id)) {
                        continue;
                    }
                    if let Some(node_id) = self.coins_creators.get(utxo_id) {
                        // TODO: Other checks
                        pool_parents.push(*node_id);
                    }
                    if utxo_validation {
                        let Some(coin) = db
                            .utxo(utxo_id)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        else {
                            return Err(Error::UtxoNotFound(*utxo_id));
                        };
                        if !coin.matches_input(input).expect("The input is coin above") {
                            return Err(Error::NotInsertedIoCoinMismatch)
                        }
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {}
                _ => {}
            }
        }
        Ok(pool_parents)
    }

    fn remove_transaction_and_dependents(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<(), Error> {
        self.remove_node_and_dependent_sub_graph(index);
        Ok(())
    }

    fn remove_transaction(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<StorageData, Error> {
        self.graph
            .remove_node(index)
            .ok_or(Error::TransactionNotFound(format!(
                "Transaction with index {:?} not found",
                index
            )))
    }
}
