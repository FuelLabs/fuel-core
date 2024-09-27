use std::{
    collections::{
        HashMap,
        HashSet,
    },
    time::Instant,
};

use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract,
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        ContractId,
        Input,
        Output,
        TxId,
        UtxoId,
    },
    services::txpool::PoolTransaction,
};
use petgraph::{
    graph::NodeIndex,
    prelude::StableDiGraph,
};

use crate::{
    collision_manager::{
        basic::BasicCollisionManagerStorage,
        CollisionReason,
    },
    error::Error,
    ports::TxPoolPersistentStorage,
    selection_algorithms::ratio_tip_gas::RatioTipGasSelectionAlgorithmStorage,
};

use super::{
    RemovedTransactions,
    Storage,
    StorageData,
};

pub struct GraphStorage {
    /// The configuration of the graph
    config: GraphConfig,
    /// The graph of transactions
    graph: StableDiGraph<StorageData, ()>,
    /// Coins -> Transaction that currently create the UTXO
    coins_creators: HashMap<UtxoId, NodeIndex>,
    /// Contract -> Transaction that currently create the contract
    contracts_creators: HashMap<ContractId, NodeIndex>,
}

pub struct GraphConfig {
    /// The maximum number of transactions per dependency chain
    pub max_txs_chain_count: usize,
}

impl GraphStorage {
    /// Create a new graph storage
    pub fn new(config: GraphConfig) -> Self {
        Self {
            config,
            graph: StableDiGraph::new(),
            coins_creators: HashMap::new(),
            contracts_creators: HashMap::new(),
        }
    }
}

impl GraphStorage {
    fn reduce_dependencies_cumulative_gas_tip_and_chain_count(
        &mut self,
        root_id: NodeIndex,
        gas_reduction: u64,
        tip_reduction: u64,
        already_visited: &mut HashSet<NodeIndex>,
    ) {
        if already_visited.contains(&root_id) {
            return;
        }
        already_visited.insert(root_id);
        let Some(root) = self.graph.node_weight_mut(root_id) else {
            debug_assert!(false, "Node with id {:?} not found", root_id);
            return;
        };
        root.dependents_cumulative_gas =
            root.dependents_cumulative_gas.saturating_sub(gas_reduction);
        root.dependents_cumulative_tip =
            root.dependents_cumulative_tip.saturating_sub(tip_reduction);
        let dependencies: Vec<_> = self.get_dependencies(root_id).collect();
        for dependency in dependencies {
            self.reduce_dependencies_cumulative_gas_tip_and_chain_count(
                dependency,
                gas_reduction,
                tip_reduction,
                already_visited,
            );
        }
    }

    /// Remove a node and all its dependent sub-graph.
    /// Edit the data of dependencies transactions accordingly.
    /// Returns the removed transactions.
    fn remove_node_and_dependent_sub_graph(
        &mut self,
        root_id: NodeIndex,
    ) -> Vec<PoolTransaction> {
        let dependencies: Vec<NodeIndex> = self.get_dependencies(root_id).collect();
        let dependents: Vec<_> = self
            .graph
            .neighbors_directed(root_id, petgraph::Direction::Outgoing)
            .collect();
        let Some(root) = self.graph.remove_node(root_id) else {
            return vec![];
        };
        let mut dependency_visited = HashSet::default();
        for dependency in dependencies {
            self.reduce_dependencies_cumulative_gas_tip_and_chain_count(
                dependency,
                root.dependents_cumulative_gas,
                root.dependents_cumulative_tip,
                &mut dependency_visited,
            );
        }
        self.clear_cache(root.transaction.outputs(), &root.transaction.id());
        let mut removed_transactions = vec![root.transaction];
        for dependent in dependents {
            removed_transactions
                .extend(self.remove_node_and_dependent_sub_graph(dependent));
        }
        removed_transactions
    }

    /// Check if the input has the right data to spend the output present in pool.
    fn check_if_coin_input_can_spend_output(
        output: &Output,
        input: &Input,
    ) -> Result<(), Error> {
        if let Input::CoinSigned(CoinSigned {
            owner,
            amount,
            asset_id,
            ..
        })
        | Input::CoinPredicate(CoinPredicate {
            owner,
            amount,
            asset_id,
            ..
        }) = input
        {
            let i_owner = owner;
            let i_amount = amount;
            let i_asset_id = asset_id;
            match output {
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    if to != i_owner {
                        return Err(Error::NotInsertedIoWrongOwner);
                    }
                    if amount != i_amount {
                        return Err(Error::NotInsertedIoWrongAmount);
                    }
                    if asset_id != i_asset_id {
                        return Err(Error::NotInsertedIoWrongAssetId);
                    }
                }
                Output::Contract(_) => return Err(Error::NotInsertedIoContractOutput),
                Output::Change { .. } => {
                    return Err(Error::NotInsertedInputDependentOnChangeOrVariable)
                }
                Output::Variable { .. } => {
                    return Err(Error::NotInsertedInputDependentOnChangeOrVariable)
                }
                Output::ContractCreated { .. } => {
                    return Err(Error::NotInsertedIoContractOutput)
                }
            };
        }
        Ok(())
    }

    /// Cache the transaction information in the storage caches.
    /// This is used to speed up the verification/dependencies searches of the transactions.
    fn cache_tx_infos(&mut self, outputs: &[Output], tx_id: &TxId, node_id: NodeIndex) {
        for (index, output) in outputs.iter().enumerate() {
            // SAFETY: We deal with CheckedTransaction there which should already check this
            let index = u16::try_from(index).expect(
                "The number of outputs in a transaction should be less than `u16::max`",
            );
            let utxo_id = UtxoId::new(*tx_id, index);
            match output {
                Output::Coin { .. } => {
                    self.coins_creators.insert(utxo_id, node_id);
                }
                Output::ContractCreated { contract_id, .. } => {
                    self.contracts_creators.insert(*contract_id, node_id);
                }
                _ => {}
            }
        }
    }

    /// Clear the caches of the storage when a transaction is removed.
    fn clear_cache(&mut self, outputs: &[Output], tx_id: &TxId) {
        for (index, output) in outputs.iter().enumerate() {
            // SAFETY: We deal with CheckedTransaction there which should already check this
            let index = u16::try_from(index).expect(
                "The number of outputs in a transaction should be less than `u16::max`",
            );
            let utxo_id = UtxoId::new(*tx_id, index);
            match output {
                Output::Coin { .. } => {
                    self.coins_creators.remove(&utxo_id);
                }
                Output::ContractCreated { contract_id, .. } => {
                    self.contracts_creators.remove(contract_id);
                }
                _ => {}
            }
        }
    }

    fn get_inner(&self, index: &NodeIndex) -> Result<&StorageData, Error> {
        self.graph
            .node_weight(*index)
            .ok_or(Error::TransactionNotFound(format!(
                "Transaction with index {:?} not found",
                index
            )))
    }

    fn get_dependents_inner(
        &self,
        index: NodeIndex,
    ) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph
            .neighbors_directed(index, petgraph::Direction::Outgoing)
    }
}

impl Storage for GraphStorage {
    type StorageIndex = NodeIndex;

    fn store_transaction(
        &mut self,
        transaction: PoolTransaction,
        dependencies: Vec<Self::StorageIndex>,
        collided_transactions: &HashSet<Self::StorageIndex>,
    ) -> Result<(Self::StorageIndex, RemovedTransactions), Error> {
        let tx_id = transaction.id();

        // Add the new transaction to the graph and update the others in consequence
        let tip = transaction.tip();
        let gas = transaction.max_gas();
        let outputs = transaction.outputs().clone();

        // Check if the dependency chain is too big
        let mut all_dependencies_recursively = HashSet::new();
        let mut to_check = dependencies.clone();
        while let Some(node_id) = to_check.pop() {
            if collided_transactions.contains(&node_id) {
                return Err(Error::Collided(
                    "Use a collided transaction as a dependency".to_string(),
                ));
            }
            // Already checked node
            if all_dependencies_recursively.contains(&node_id) {
                continue;
            }
            let Some(dependency_node) = self.graph.node_weight(node_id) else {
                return Err(Error::Storage(format!(
                    "Node with id {:?} not found",
                    node_id
                )));
            };
            if dependency_node.number_dependents_in_chain
                >= self.config.max_txs_chain_count
            {
                return Err(Error::NotInsertedChainDependencyTooBig);
            }
            all_dependencies_recursively.insert(node_id);
            if all_dependencies_recursively.len() >= self.config.max_txs_chain_count {
                return Err(Error::NotInsertedChainDependencyTooBig);
            }
            to_check.extend(self.get_dependencies(node_id));
        }

        // Remove collisions and their dependencies from the graph
        let mut removed_transactions = vec![];
        for collision in collided_transactions {
            removed_transactions
                .extend(self.remove_node_and_dependent_sub_graph(*collision));
        }

        let node = StorageData {
            dependents_cumulative_tip: tip,
            dependents_cumulative_gas: gas,
            transaction,
            number_dependents_in_chain: 0,
        };

        // Add the transaction to the graph
        let node_id = self.graph.add_node(node);
        for dependency in dependencies {
            self.graph.add_edge(dependency, node_id, ());
        }
        self.cache_tx_infos(&outputs, &tx_id, node_id);

        // Update the cumulative tip and gas of the dependencies transactions and recursively their dependencies, etc.
        for node_id in all_dependencies_recursively {
            let Some(node) = self.graph.node_weight_mut(node_id) else {
                return Err(Error::Storage(format!(
                    "Node with id {:?} not found",
                    node_id
                )));
            };
            node.number_dependents_in_chain =
                node.number_dependents_in_chain.saturating_add(1);
            node.dependents_cumulative_tip =
                node.dependents_cumulative_tip.saturating_add(tip);
            node.dependents_cumulative_gas =
                node.dependents_cumulative_gas.saturating_add(gas);
        }
        Ok((node_id, removed_transactions))
    }

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error> {
        self.get_inner(index)
    }

    fn get_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex> {
        self.graph
            .neighbors_directed(index, petgraph::Direction::Incoming)
    }

    fn get_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex> {
        self.get_dependents_inner(index)
    }

    fn validate_inputs(
        &self,
        transaction: &PoolTransaction,
        persistent_storage: &impl TxPoolPersistentStorage,
        utxo_validation: bool,
    ) -> Result<(), Error> {
        for input in transaction.inputs() {
            match input {
                // If the utxo is created in the pool, need to check if we don't spend too much (utxo can still be unresolved)
                // If the utxo_validation is active, we need to check if the utxo exists in the database and is valid
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    if let Some(node_id) = self.coins_creators.get(utxo_id) {
                        let Some(node) = self.graph.node_weight(*node_id) else {
                            return Err(Error::Storage(format!(
                                "Node with id {:?} not found",
                                node_id
                            )));
                        };
                        let output =
                            &node.transaction.outputs()[utxo_id.output_index() as usize];
                        Self::check_if_coin_input_can_spend_output(output, input)?;
                    } else if utxo_validation {
                        let Some(coin) = persistent_storage
                            .utxo(utxo_id)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        else {
                            return Err(Error::UtxoNotFound(*utxo_id));
                        };
                        if !coin.matches_input(input).expect("The input is coin above") {
                            return Err(Error::NotInsertedIoCoinMismatch);
                        }
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // since message id is derived, we don't need to double check all the fields
                    // Maybe this should be on an other function as it's not a dependency finder but just a test
                    if utxo_validation {
                        if let Some(db_message) = persistent_storage
                            .message(nonce)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        {
                            // verify message id integrity
                            if !db_message
                                .matches_input(input)
                                .expect("Input is a message above")
                            {
                                return Err(Error::NotInsertedIoMessageMismatch);
                            }
                        } else {
                            return Err(Error::NotInsertedInputMessageUnknown(*nonce));
                        }
                    }
                }
                Input::Contract(Contract { contract_id, .. }) => {
                    if !self.contracts_creators.contains_key(contract_id)
                        && !persistent_storage
                            .contract_exist(contract_id)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                    {
                        return Err(Error::NotInsertedInputContractDoesNotExist(
                            *contract_id,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn collect_transaction_dependencies(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<Vec<Self::StorageIndex>, Error> {
        let mut pool_dependencies = Vec::new();
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    if let Some(node_id) = self.coins_creators.get(utxo_id) {
                        pool_dependencies.push(*node_id);
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {}
                Input::Contract(Contract { contract_id, .. }) => {
                    if let Some(node_id) = self.contracts_creators.get(contract_id) {
                        pool_dependencies.push(*node_id);
                    }
                }
            }
        }
        Ok(pool_dependencies)
    }

    fn remove_transaction_without_dependencies(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<StorageData, Error> {
        if self.get_dependencies(index).next().is_some() {
            return Err(Error::Storage("Tried to remove a transaction without dependencies but it has dependencies".to_string()));
        }
        self.graph
            .remove_node(index)
            .ok_or(Error::TransactionNotFound(format!(
                "Transaction with index {:?} not found",
                index
            )))
            .inspect(|node| {
                self.clear_cache(node.transaction.outputs(), &node.transaction.id());
            })
    }

    fn count(&self) -> u64 {
        self.graph.node_count() as u64
    }
}

impl BasicCollisionManagerStorage for GraphStorage {
    type StorageIndex = NodeIndex;

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error> {
        self.get_inner(index)
    }
}

impl RatioTipGasSelectionAlgorithmStorage for GraphStorage {
    type StorageIndex = NodeIndex;

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error> {
        self.get_inner(index)
    }

    fn get_dependents(
        &self,
        index: &Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex> {
        self.get_dependents_inner(*index)
    }
}
