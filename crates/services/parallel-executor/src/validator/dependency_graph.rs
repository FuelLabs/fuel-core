use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ContractId,
        Input,
        Transaction,
    },
};

/// Dependency graph for contract-based transactions
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Contract ID -> Set of transaction indices that use this contract
    contract_to_transactions: HashMap<ContractId, HashSet<usize>>,
    /// Transaction index -> Set of contract IDs it depends on
    transaction_to_contracts: HashMap<usize, HashSet<ContractId>>,
    /// Transaction index -> Transaction
    transactions: HashMap<usize, Transaction>,
    /// Tracks remaining dependencies for each transaction
    remaining_dependencies: HashMap<usize, usize>,
    /// Cache of currently independent transactions
    independent_transactions: HashSet<usize>,
    /// Queue for newly independent transactions
    newly_independent_queue: VecDeque<usize>,
}

impl DependencyGraph {
    pub fn new(capacity: usize) -> Self {
        Self {
            contract_to_transactions: HashMap::new(),
            transaction_to_contracts: HashMap::with_capacity(capacity),
            transactions: HashMap::with_capacity(capacity),
            remaining_dependencies: HashMap::with_capacity(capacity),
            independent_transactions: HashSet::with_capacity(capacity),
            newly_independent_queue: VecDeque::with_capacity(capacity),
        }
    }

    pub fn add_transaction(&mut self, index: usize, transaction: Transaction) {
        let mut contract_deps = HashSet::new();

        // extract contract dependencies
        for input in transaction.inputs().iter() {
            if let Input::Contract(contract_input) = input {
                let contract_id = contract_input.contract_id;
                contract_deps.insert(contract_id);

                // add to contract -> transactions mapping
                self.contract_to_transactions
                    .entry(contract_id)
                    .or_default()
                    .insert(index);
            }
        }

        let dep_count = contract_deps.len();
        self.transaction_to_contracts.insert(index, contract_deps);
        self.transactions.insert(index, transaction);
        self.remaining_dependencies.insert(index, dep_count);

        // if no dependencies, immediately mark as independent
        if dep_count == 0 {
            self.independent_transactions.insert(index);
            self.newly_independent_queue.push_back(index);
        }
    }

    /// Batch add multiple transactions for better performance
    pub fn add_transactions<I>(&mut self, transactions: I)
    where
        I: IntoIterator<Item = (usize, Transaction)>,
    {
        for (index, transaction) in transactions {
            self.add_transaction(index, transaction);
        }
    }

    /// Get transactions that are currently independent
    pub fn get_independent_transactions(&self) -> Vec<usize> {
        self.independent_transactions.iter().copied().collect()
    }

    /// Pop next independent transaction from queue
    pub fn pop_independent_transaction(&mut self) -> Option<usize> {
        self.newly_independent_queue.pop_front()
    }

    /// Check if there are any independent transactions ready
    pub fn has_independent_transactions(&self) -> bool {
        !self.independent_transactions.is_empty()
    }

    /// Remove a transaction and update dependencies
    pub fn remove_transaction(&mut self, tx_index: usize) -> Option<Vec<usize>> {
        // remove the transaction from our tracking
        self.transactions.remove(&tx_index)?;
        let contracts = self.transaction_to_contracts.remove(&tx_index)?;
        self.remaining_dependencies.remove(&tx_index);

        // remove from independent set if it was there
        self.independent_transactions.remove(&tx_index);

        let mut newly_independent = Vec::new();

        // pre-collect transactions that might be affected
        let mut potentially_affected = HashSet::new();
        for &contract_id in &contracts {
            if let Some(tx_set) = self.contract_to_transactions.get(&contract_id) {
                potentially_affected.extend(tx_set.iter().copied());
            }
        }

        // remove this transaction from affected contracts
        for contract_id in contracts {
            if let Some(tx_set) = self.contract_to_transactions.get_mut(&contract_id) {
                tx_set.remove(&tx_index);

                if tx_set.is_empty() {
                    self.contract_to_transactions.remove(&contract_id);
                }
            }

            // only check potentially affected transactions
            for &other_tx_idx in &potentially_affected {
                if other_tx_idx == tx_index {
                    continue; // skip the removed transaction
                }

                if let Some(remaining_deps) =
                    self.remaining_dependencies.get_mut(&other_tx_idx)
                {
                    if let Some(other_contracts) =
                        self.transaction_to_contracts.get(&other_tx_idx)
                    {
                        if other_contracts.contains(&contract_id) {
                            *remaining_deps -= 1;
                            if *remaining_deps == 0 {
                                // mark as independent
                                self.independent_transactions.insert(other_tx_idx);
                                self.newly_independent_queue.push_back(other_tx_idx);
                                newly_independent.push(other_tx_idx);

                                // clean up dependency tracking for this now-independent transaction
                                if let Some(contracts) =
                                    self.transaction_to_contracts.remove(&other_tx_idx)
                                {
                                    for contract in contracts {
                                        if let Some(tx_set) = self
                                            .contract_to_transactions
                                            .get_mut(&contract)
                                        {
                                            tx_set.remove(&other_tx_idx);
                                            if tx_set.is_empty() {
                                                self.contract_to_transactions
                                                    .remove(&contract);
                                            }
                                        }
                                    }
                                }
                                self.remaining_dependencies.remove(&other_tx_idx);
                            }
                        }
                    }
                }
            }
        }

        Some(newly_independent)
    }

    /// Batch remove multiple transactions
    pub fn remove_transactions(&mut self, tx_indices: &[usize]) -> Vec<usize> {
        let mut all_newly_independent = Vec::new();

        for &tx_index in tx_indices {
            if let Some(newly_independent) = self.remove_transaction(tx_index) {
                all_newly_independent.extend(newly_independent);
            }
        }

        all_newly_independent
    }

    pub fn get_transaction(&self, index: usize) -> Option<&Transaction> {
        self.transactions.get(&index)
    }

    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        fuel_tx::{
            ContractId,
            Input,
            Transaction,
            TransactionBuilder,
            UniqueIdentifier,
            UtxoId,
        },
        fuel_types::ChainId,
    };

    // Helper function to create a transaction with contract inputs
    fn create_transaction_with_contracts(contract_ids: Vec<ContractId>) -> Transaction {
        let mut builder = TransactionBuilder::script(vec![], vec![]);

        for contract_id in contract_ids {
            builder.add_input(Input::contract(
                UtxoId::new([0u8; 32].into(), 0),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id,
            ));
        }

        builder.finalize_as_transaction()
    }

    // Helper function to create a transaction without contracts
    fn create_simple_transaction() -> Transaction {
        TransactionBuilder::script(vec![], vec![]).finalize_as_transaction()
    }

    #[test]
    fn new__creates_empty_graph() {
        let graph = DependencyGraph::new(0);

        assert!(graph.is_empty());
        assert!(graph.get_independent_transactions().is_empty());
    }

    #[test]
    fn add_transaction__with_no_contracts__creates_independent_transaction() {
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        graph.add_transaction(0, tx);

        assert!(!graph.is_empty());
        assert_eq!(graph.get_independent_transactions(), vec![0]);
        assert!(graph.get_transaction(0).is_some());
    }

    #[test]
    fn add_transaction__with_single_contract__creates_dependent_transaction() {
        let mut graph = DependencyGraph::new(1);
        let contract_id = ContractId::from([1u8; 32]);
        let tx = create_transaction_with_contracts(vec![contract_id]);

        graph.add_transaction(0, tx);

        assert!(!graph.is_empty());
        assert!(graph.get_independent_transactions().is_empty()); // Should be dependent
        assert!(graph.get_transaction(0).is_some());
    }

    #[test]
    fn add_transaction__with_multiple_contracts__tracks_all_dependencies() {
        let mut graph = DependencyGraph::new(1);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);
        let tx = create_transaction_with_contracts(vec![contract_id1, contract_id2]);

        graph.add_transaction(0, tx);

        assert!(!graph.is_empty());
        assert!(graph.get_independent_transactions().is_empty()); // Should have 2 dependencies

        // Check internal state
        assert_eq!(graph.remaining_dependencies.get(&0), Some(&2));
        assert!(
            graph
                .contract_to_transactions
                .get(&contract_id1)
                .unwrap()
                .contains(&0)
        );
        assert!(
            graph
                .contract_to_transactions
                .get(&contract_id2)
                .unwrap()
                .contains(&0)
        );
    }

    #[test]
    fn multiple_transactions__same_contract__both_tracked() {
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);
        let tx1 = create_transaction_with_contracts(vec![contract_id]);
        let tx2 = create_transaction_with_contracts(vec![contract_id]);

        graph.add_transaction(0, tx1);
        graph.add_transaction(1, tx2);

        assert!(!graph.is_empty());
        assert!(graph.get_independent_transactions().is_empty());

        // Both transactions should be tracked under the same contract
        let tx_set = graph.contract_to_transactions.get(&contract_id).unwrap();
        assert!(tx_set.contains(&0));
        assert!(tx_set.contains(&1));
    }

    #[test]
    fn remove_transaction__nonexistent__returns_none() {
        let mut graph = DependencyGraph::new(0);

        let result = graph.remove_transaction(999);

        assert!(result.is_none());
    }

    #[test]
    fn remove_transaction__independent__removes_successfully() {
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();
        graph.add_transaction(0, tx);

        let newly_independent = graph.remove_transaction(0).unwrap();

        assert!(graph.is_empty());
        assert!(newly_independent.is_empty());
        assert!(graph.get_transaction(0).is_none());
    }

    #[test]
    fn remove_transaction__with_contract__makes_dependent_independent() {
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);

        // Create two transactions both using the same contract (competing for it)
        let tx1 = create_transaction_with_contracts(vec![contract_id]);
        let tx2 = create_transaction_with_contracts(vec![contract_id]);

        graph.add_transaction(0, tx1);
        graph.add_transaction(1, tx2);

        // Initially both are dependent (both waiting for the contract)
        assert!(graph.get_independent_transactions().is_empty());

        // Remove the first transaction - this should free up the contract
        let newly_independent = graph.remove_transaction(0).unwrap();

        // The second transaction should now be independent since the contract is no longer contested
        assert_eq!(newly_independent, vec![1]);
        assert_eq!(graph.get_independent_transactions(), vec![1]);
    }

    #[test]
    fn remove_transaction__complex_dependency_chain__updates_correctly() {
        let mut graph = DependencyGraph::new(3);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);

        // TX0: Uses contract1
        // TX1: Uses contract1 and contract2
        // TX2: Uses contract2
        let tx0 = create_transaction_with_contracts(vec![contract_id1]);
        let tx1 = create_transaction_with_contracts(vec![contract_id1, contract_id2]);
        let tx2 = create_transaction_with_contracts(vec![contract_id2]);

        graph.add_transaction(0, tx0);
        graph.add_transaction(1, tx1);
        graph.add_transaction(2, tx2);

        // All should be dependent initially
        assert!(graph.get_independent_transactions().is_empty());

        // Remove TX0 (uses contract1)
        let newly_independent = graph.remove_transaction(0).unwrap();

        // TX1 should have one less dependency but still be dependent
        // TX2 should be unaffected
        // No one should become independent yet
        assert!(newly_independent.is_empty());
        assert!(graph.get_independent_transactions().is_empty());

        // Remove TX2 (uses contract2)
        let newly_independent = graph.remove_transaction(2).unwrap();

        // Now TX1 should become independent (no more dependencies)
        assert_eq!(newly_independent, vec![1]);
        assert_eq!(graph.get_independent_transactions(), vec![1]);
    }

    #[test]
    fn remove_transaction__multiple_contracts_same_transaction__handles_correctly() {
        let mut graph = DependencyGraph::new(1);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);

        // One transaction uses multiple contracts
        let tx = create_transaction_with_contracts(vec![contract_id1, contract_id2]);
        graph.add_transaction(0, tx);

        assert_eq!(graph.remaining_dependencies.get(&0), Some(&2));

        // Remove the transaction
        let newly_independent = graph.remove_transaction(0).unwrap();

        assert!(graph.is_empty());
        assert!(newly_independent.is_empty());
        assert!(!graph.contract_to_transactions.contains_key(&contract_id1));
        assert!(!graph.contract_to_transactions.contains_key(&contract_id2));
    }

    #[test]
    fn get_transaction__existing__returns_transaction() {
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();
        let tx_id = tx.id(&ChainId::default());

        graph.add_transaction(0, tx);

        let retrieved_tx = graph.get_transaction(0).unwrap();
        assert_eq!(retrieved_tx.id(&ChainId::default()), tx_id);
    }

    #[test]
    fn get_transaction__nonexistent__returns_none() {
        let graph = DependencyGraph::new(0);

        assert!(graph.get_transaction(0).is_none());
    }

    #[test]
    fn is_empty__empty_graph__returns_true() {
        let graph = DependencyGraph::new(0);
        assert!(graph.is_empty());
    }

    #[test]
    fn is_empty__with_transactions__returns_false() {
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        graph.add_transaction(0, tx);

        assert!(!graph.is_empty());
    }

    #[test]
    fn is_empty__after_removing_all_transactions__returns_true() {
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        graph.add_transaction(0, tx);
        assert!(!graph.is_empty());

        graph.remove_transaction(0);
        assert!(graph.is_empty());
    }

    #[test]
    fn get_independent_transactions__mixed_dependencies__returns_only_independent() {
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);

        // Add independent transaction
        let independent_tx = create_simple_transaction();
        graph.add_transaction(0, independent_tx);

        // Add dependent transaction
        let dependent_tx = create_transaction_with_contracts(vec![contract_id]);
        graph.add_transaction(1, dependent_tx);

        let independent = graph.get_independent_transactions();
        assert_eq!(independent.len(), 1);
        assert!(independent.contains(&0));
        assert!(!independent.contains(&1));
    }

    #[test]
    fn dependency_resolution__realistic_scenario() {
        let mut graph = DependencyGraph::new(5);
        let contract_a = ContractId::from([1u8; 32]);
        let contract_b = ContractId::from([2u8; 32]);
        let contract_c = ContractId::from([3u8; 32]);

        // Create a realistic dependency scenario:
        // TX0: Independent (no contracts)
        // TX1: Uses contract A
        // TX2: Uses contract A and B
        // TX3: Uses contract B and C
        // TX4: Uses contract C

        graph.add_transaction(0, create_simple_transaction());
        graph.add_transaction(1, create_transaction_with_contracts(vec![contract_a]));
        graph.add_transaction(
            2,
            create_transaction_with_contracts(vec![contract_a, contract_b]),
        );
        graph.add_transaction(
            3,
            create_transaction_with_contracts(vec![contract_b, contract_c]),
        );
        graph.add_transaction(4, create_transaction_with_contracts(vec![contract_c]));

        // Initially only TX0 should be independent
        assert_eq!(graph.get_independent_transactions(), vec![0]);

        // Remove TX0
        graph.remove_transaction(0);

        // Still no new independent transactions
        assert!(graph.get_independent_transactions().is_empty());

        // Remove TX1 (contract A)
        graph.remove_transaction(1);

        // TX2 should have one less dependency but still not independent
        assert!(graph.get_independent_transactions().is_empty());

        // Remove TX4 (contract C)
        graph.remove_transaction(4);

        // TX3 should have one less dependency but still not independent
        assert!(graph.get_independent_transactions().is_empty());

        // Remove TX2 (contracts A and B)
        let newly_independent = graph.remove_transaction(2).unwrap();

        // TX3 should now be independent (only had contract B and C dependencies)
        assert_eq!(newly_independent, vec![3]);
        assert_eq!(graph.get_independent_transactions(), vec![3]);
    }
}
