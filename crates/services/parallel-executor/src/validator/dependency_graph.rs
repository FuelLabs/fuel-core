use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ContractId,
        Input,
        Output,
        Transaction,
    },
};
use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use crate::coin::CoinInBatch;

/// Dependency graph for contract-based transactions
#[derive(Debug)]
pub struct DependencyGraph {
    /// Contract ID -> Ordered list of transaction indices that use this contract (first is ready)
    contract_to_transactions: HashMap<ContractId, VecDeque<usize>>,
    contract_to_changes: HashMap<ContractId, Changes>,
    /// Transaction index -> Set of contract IDs it depends on
    transaction_to_contracts: HashMap<usize, HashSet<ContractId>>,
    /// Transaction index -> Transaction
    transactions: HashMap<usize, (Transaction, Vec<CoinInBatch>)>,
    /// Cache of currently ready transactions
    ready_transactions: HashSet<usize>,
    /// Queue for newly ready transactions
    newly_ready_queue: VecDeque<usize>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DependencyGraph {
    pub fn new(capacity: usize) -> Self {
        Self {
            contract_to_transactions: HashMap::new(),
            contract_to_changes: HashMap::new(),
            transaction_to_contracts: HashMap::with_capacity(capacity),
            transactions: HashMap::with_capacity(capacity),
            ready_transactions: HashSet::with_capacity(capacity),
            newly_ready_queue: VecDeque::with_capacity(capacity),
        }
    }

    pub fn add_transaction(&mut self, index: usize, transaction: Transaction) {
        let mut contract_deps = HashSet::new();
        let mut coins_used = Vec::new();
        let mut is_ready = true;

        let mut handle_contract = |contract_id: ContractId| {
            contract_deps.insert(contract_id);

            // add to contract -> transactions mapping (ordered by insertion)
            let tx_queue = self
                .contract_to_transactions
                .entry(contract_id)
                .or_insert_with(VecDeque::new);

            // if this is not the first transaction for this contract, it's not ready
            if !tx_queue.is_empty() {
                is_ready = false;
            }

            tx_queue.push_back(index);
            self.contract_to_changes
                .entry(contract_id)
                .or_insert_with(|| Changes::default());
        };

        // Extract contract dependencies
        for input in transaction.inputs().iter() {
            match input {
                Input::Contract(contract_input) => {
                    handle_contract(contract_input.contract_id);
                }
                fuel_core_types::fuel_tx::Input::CoinSigned(coin) => {
                    coins_used.push(CoinInBatch::from_signed_coin(
                        coin,
                        index,
                        Default::default(),
                    ));
                }
                fuel_core_types::fuel_tx::Input::CoinPredicate(coin) => {
                    coins_used.push(CoinInBatch::from_predicate_coin(
                        coin,
                        index,
                        Default::default(),
                    ));
                }
                _ => {}
            }
        }

        for output in transaction.outputs().iter() {
            if let Output::ContractCreated { contract_id, .. } = output {
                handle_contract(*contract_id);
            }
        }

        self.transaction_to_contracts.insert(index, contract_deps);
        self.transactions.insert(index, (transaction, coins_used));

        // if no contracts or first to use all contracts, mark as ready
        if is_ready {
            self.ready_transactions.insert(index);
            self.newly_ready_queue.push_back(index);
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

    /// Get transactions that are currently ready
    pub fn get_ready_transactions(&self) -> Vec<usize> {
        self.ready_transactions.iter().copied().collect()
    }

    /// Pop next ready transaction from queue
    pub fn pop_ready_transaction(&mut self) -> Option<usize> {
        self.newly_ready_queue.pop_front()
    }

    /// Check if there are any ready transactions
    pub fn has_ready_transactions(&self) -> bool {
        !self.ready_transactions.is_empty()
    }

    /// Remove a transaction and update dependencies
    pub fn mark_tx_as_executed(
        &mut self,
        tx_index: usize,
        changes: Changes,
    ) -> Option<Vec<usize>> {
        // remove the transaction from our tracking
        let contracts = self.transaction_to_contracts.remove(&tx_index)?;

        // remove from ready set if it was there
        self.ready_transactions.remove(&tx_index);

        let mut newly_ready = Vec::new();
        let mut contracts_to_clean = Vec::new();

        // for each contract this transaction used
        for contract_id in contracts {
            if let Some(tx_queue) = self.contract_to_transactions.get_mut(&contract_id) {
                // find and remove this transaction from the queue
                if let Some(pos) = tx_queue.iter().position(|&x| x == tx_index) {
                    tx_queue.remove(pos);

                    // if this was the first (ready) transaction, make the next one ready
                    if pos == 0 && !tx_queue.is_empty() {
                        let next_tx = tx_queue[0];
                        newly_ready.push(next_tx);
                    }
                }

                // mark empty contract queues for cleanup
                if tx_queue.is_empty() {
                    contracts_to_clean.push(contract_id);
                }
            }
            // Clone because a change for a contract made on a previous transaction and be used by multiple transactions afterwards
            self.contract_to_changes
                .insert(contract_id, changes.clone());
        }

        // clean up empty contract queues
        for contract_id in contracts_to_clean {
            self.contract_to_transactions.remove(&contract_id);
        }

        // now check which of the newly ready candidates are actually ready for ALL their contracts
        let mut actually_ready = Vec::new();
        for &next_tx in &newly_ready {
            if let Some(next_contracts) = self.transaction_to_contracts.get(&next_tx) {
                let is_now_ready = next_contracts.iter().all(|&contract| {
                    self.contract_to_transactions
                        .get(&contract)
                        .map(|queue| queue.front() == Some(&next_tx))
                        .unwrap_or(false)
                });

                if is_now_ready && !self.ready_transactions.contains(&next_tx) {
                    self.ready_transactions.insert(next_tx);
                    self.newly_ready_queue.push_back(next_tx);
                    actually_ready.push(next_tx);
                }
            }
        }

        Some(actually_ready)
    }

    pub fn extract_transaction_object(
        &mut self,
        index: usize,
    ) -> Option<((Transaction, Vec<CoinInBatch>), Changes)> {
        let mut changes = Changes::default();
        if let Some(transaction) = self.transactions.remove(&index) {
            let contracts = self
                .transaction_to_contracts
                .remove(&index)
                .expect("Transaction should be present in the graph");
            for contract in contracts {
                // Remove is fine because the contract can't be used by any other transaction before `mark_tx_as_executed` is called
                // and repopulate the changes.
                changes.extend(
                    self.contract_to_changes
                        .remove(&contract)
                        .expect("Contract should have changes associated, even empty"),
                );
            }
            Some((transaction, changes))
        } else {
            None
        }
    }

    pub fn get_transaction(
        &self,
        index: usize,
    ) -> Option<&(Transaction, Vec<CoinInBatch>)> {
        self.transactions.get(&index)
    }

    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    /// Clear all data and reset to empty state
    pub fn clear(&mut self) {
        self.contract_to_transactions.clear();
        self.transaction_to_contracts.clear();
        self.transactions.clear();
        self.ready_transactions.clear();
        self.newly_ready_queue.clear();
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
        // given
        let graph = DependencyGraph::new(0);

        // then
        assert!(graph.is_empty());
        assert!(graph.get_ready_transactions().is_empty());
    }

    #[test]
    fn add_transaction__with_no_contracts__creates_independent_transaction() {
        // given
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        // when
        graph.add_transaction(0, tx);

        // then
        assert!(!graph.is_empty());
        assert_eq!(graph.get_ready_transactions(), vec![0]);
        assert!(graph.get_transaction(0).is_some());
    }

    #[test]
    fn add_transaction__with_single_contract__creates_dependent_transaction() {
        // given
        let mut graph = DependencyGraph::new(1);
        let contract_id = ContractId::from([1u8; 32]);
        let tx = create_transaction_with_contracts(vec![contract_id]);

        // when
        graph.add_transaction(0, tx);

        // then
        assert!(!graph.is_empty());
        assert_eq!(graph.get_ready_transactions(), vec![0]); // First to use contract is ready
        assert!(graph.get_transaction(0).is_some());
    }

    #[test]
    fn add_transaction__with_multiple_contracts__tracks_all_dependencies() {
        // given
        let mut graph = DependencyGraph::new(1);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);
        let tx = create_transaction_with_contracts(vec![contract_id1, contract_id2]);

        // when
        graph.add_transaction(0, tx);

        // then
        assert!(!graph.is_empty());
        assert_eq!(graph.get_ready_transactions(), vec![0]); // First to use both contracts is ready

        // Check internal state - both contracts should have this transaction as first
        assert_eq!(
            graph
                .contract_to_transactions
                .get(&contract_id1)
                .unwrap()
                .front(),
            Some(&0)
        );
        assert_eq!(
            graph
                .contract_to_transactions
                .get(&contract_id2)
                .unwrap()
                .front(),
            Some(&0)
        );
    }

    #[test]
    fn multiple_transactions__same_contract__both_tracked() {
        // given
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);
        let tx1 = create_transaction_with_contracts(vec![contract_id]);
        let tx2 = create_transaction_with_contracts(vec![contract_id]);

        // when
        graph.add_transaction(0, tx1);
        graph.add_transaction(1, tx2);

        assert!(!graph.is_empty());
        assert_eq!(graph.get_ready_transactions(), vec![0]); // Only first is ready

        // then: both transactions should be tracked under the same contract
        let tx_queue = graph.contract_to_transactions.get(&contract_id).unwrap();
        assert_eq!(tx_queue.len(), 2);
        assert_eq!(tx_queue.front(), Some(&0)); // First transaction
        assert_eq!(tx_queue.back(), Some(&1)); // Second transaction
    }

    #[test]
    fn remove_transaction__nonexistent__returns_none() {
        // given
        let mut graph = DependencyGraph::new(0);

        // when
        let result = graph.mark_tx_as_executed(999, Default::default());

        // then
        assert!(result.is_none());
    }

    #[test]
    fn remove_transaction__independent__removes_successfully() {
        // given
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();
        graph.add_transaction(0, tx);

        // when
        let newly_ready = graph.mark_tx_as_executed(0, Default::default()).unwrap();

        // then
        assert!(graph.is_empty());
        assert!(newly_ready.is_empty());
        assert!(graph.get_transaction(0).is_none());
    }

    #[test]
    fn remove_transaction__with_contract__makes_dependent_independent() {
        // given
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);

        // Create two transactions both using the same contract
        let tx1 = create_transaction_with_contracts(vec![contract_id]);
        let tx2 = create_transaction_with_contracts(vec![contract_id]);

        // when
        graph.add_transaction(0, tx1);
        graph.add_transaction(1, tx2);

        // Initially only first transaction is ready
        assert_eq!(graph.get_ready_transactions(), vec![0]);

        // Remove the first transaction - this should make the second one ready
        let newly_ready = graph.mark_tx_as_executed(0, Default::default()).unwrap();

        // then: the second transaction should now be ready
        assert_eq!(newly_ready, vec![1]);
        assert_eq!(graph.get_ready_transactions(), vec![1]);
    }

    #[test]
    fn remove_transaction__complex_dependency_chain__updates_correctly() {
        // given
        let mut graph = DependencyGraph::new(3);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);

        // TX0: Uses contract1 (ready - first to use contract1)
        // TX1: Uses contract1 and contract2 (not ready - second to use contract1, first to use contract2 but needs both)
        // TX2: Uses contract2 (ready - first to use contract2)
        let tx0 = create_transaction_with_contracts(vec![contract_id1]);
        let tx1 = create_transaction_with_contracts(vec![contract_id1, contract_id2]);
        let tx2 = create_transaction_with_contracts(vec![contract_id2]);

        // when
        graph.add_transaction(0, tx0);
        graph.add_transaction(1, tx1);
        graph.add_transaction(2, tx2);

        // TX0 should be ready (first to use contract1)
        let mut ready = graph.get_ready_transactions();
        ready.sort();
        assert_eq!(ready, vec![0]);

        // Remove TX0 (uses contract1)
        let newly_ready = graph.mark_tx_as_executed(0, Default::default()).unwrap();

        // TX1 is ready now
        assert_eq!(newly_ready, vec![1]);

        // Remove TX2 (uses contract2)
        let _ = graph.mark_tx_as_executed(2, Default::default()).unwrap();

        // then: TX1 should become ready (first in line for both contracts)
        assert_eq!(graph.get_ready_transactions(), vec![1]);
    }

    #[test]
    fn remove_transaction__multiple_contracts_same_transaction__handles_correctly() {
        // given
        let mut graph = DependencyGraph::new(1);
        let contract_id1 = ContractId::from([1u8; 32]);
        let contract_id2 = ContractId::from([2u8; 32]);

        // when: One transaction uses multiple contracts (ready - first to use both)
        let tx = create_transaction_with_contracts(vec![contract_id1, contract_id2]);
        graph.add_transaction(0, tx);

        assert_eq!(graph.get_ready_transactions(), vec![0]);

        // Remove the transaction
        let newly_ready = graph.mark_tx_as_executed(0, Default::default()).unwrap();

        // then: graph should be empty, no newly ready transactions
        assert!(graph.is_empty());
        assert!(newly_ready.is_empty());
        assert!(!graph.contract_to_transactions.contains_key(&contract_id1));
        assert!(!graph.contract_to_transactions.contains_key(&contract_id2));
    }

    #[test]
    fn get_transaction__existing__returns_transaction() {
        // given
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();
        let tx_id = tx.id(&ChainId::default());

        // when
        graph.add_transaction(0, tx);

        let (retrieved_tx, _) = graph.get_transaction(0).unwrap();
        // then
        assert_eq!(retrieved_tx.id(&ChainId::default()), tx_id);
    }

    #[test]
    fn get_transaction__nonexistent__returns_none() {
        // given
        let graph = DependencyGraph::new(0);

        // then
        assert!(graph.get_transaction(0).is_none());
    }

    #[test]
    fn is_empty__empty_graph__returns_true() {
        // given
        let graph = DependencyGraph::new(0);
        // then
        assert!(graph.is_empty());
    }

    #[test]
    fn is_empty__with_transactions__returns_false() {
        // given
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        // when
        graph.add_transaction(0, tx);

        // then
        assert!(!graph.is_empty());
    }

    #[test]
    fn is_empty__after_removing_all_transactions__returns_true() {
        // given
        let mut graph = DependencyGraph::new(1);
        let tx = create_simple_transaction();

        // when
        graph.add_transaction(0, tx);
        assert!(!graph.is_empty());

        graph.mark_tx_as_executed(0, Default::default());
        // then
        assert!(graph.is_empty());
    }

    #[test]
    fn get_independent_transactions__mixed_dependencies__returns_only_independent() {
        // given
        let mut graph = DependencyGraph::new(2);
        let contract_id = ContractId::from([1u8; 32]);

        // Add transaction without contracts (ready)
        let independent_tx = create_simple_transaction();
        graph.add_transaction(0, independent_tx);

        // when: Add transaction with contract (ready - first to use contract)
        let dependent_tx = create_transaction_with_contracts(vec![contract_id]);
        graph.add_transaction(1, dependent_tx);

        // then: Only independent transactions should be ready
        let ready = graph.get_ready_transactions();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&0));
        assert!(ready.contains(&1)); // Both are ready now
    }

    #[test]
    fn dependency_resolution__realistic_scenario() {
        // given
        let mut graph = DependencyGraph::new(5);
        let contract_a = ContractId::from([1u8; 32]);
        let contract_b = ContractId::from([2u8; 32]);
        let contract_c = ContractId::from([3u8; 32]);

        // when: Create a realistic dependency scenario:
        // TX0: No contracts (ready)
        // TX1: Uses contract A (ready - first to use A)
        // TX2: Uses contract A and B (not ready - second to use A, first to use B but needs both)
        // TX3: Uses contract B and C (not ready - second to use B, first to use C but needs both)
        // TX4: Uses contract C (ready - first to use C)

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

        // Initially TX0 and TX1 should be ready
        let mut ready = graph.get_ready_transactions();
        ready.sort();
        assert_eq!(ready, vec![0, 1]);

        // Remove TX0
        graph.mark_tx_as_executed(0, Default::default());

        // then
        // Still TX1 ready
        let mut ready = graph.get_ready_transactions();
        ready.sort();
        assert_eq!(ready, vec![1]);

        // Remove TX1 (contract A)
        let newly_ready = graph.mark_tx_as_executed(1, Default::default()).unwrap();

        // TX2 should now be ready (first in line for contract A, and contract B is free)
        assert_eq!(newly_ready, vec![2]);

        // Remove TX4 (contract C)
        let newly_ready = graph.mark_tx_as_executed(4, Default::default()).unwrap();

        // TX3 still can't be ready because it needs contract B and there's TX2 ahead in line for B
        assert!(newly_ready.is_empty());

        // Remove TX2 (contracts A and B)
        let newly_ready = graph.mark_tx_as_executed(2, Default::default()).unwrap();

        // TX3 should now be ready (first in line for contract B, and contract C is free)
        assert_eq!(newly_ready, vec![3]);
        assert_eq!(graph.get_ready_transactions(), vec![3]);
    }
}
