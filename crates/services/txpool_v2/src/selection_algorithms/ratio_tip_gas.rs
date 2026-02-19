use std::{
    cmp::{
        Ordering,
        Reverse,
    },
    collections::{
        BTreeMap,
        VecDeque,
    },
    fmt::Debug,
    time::SystemTime,
};

use fuel_core_types::fuel_tx::TxId;
use num_rational::Ratio;

use crate::storage::{
    RemovedTransactions,
    StorageData,
};

use super::{
    Constraints,
    SelectionAlgorithm,
};

#[cfg(test)]
use fuel_core_types::services::txpool::ArcPoolTx;

#[cfg(test)]
use std::collections::HashMap;

pub trait RatioTipGasSelectionAlgorithmStorage {
    type StorageIndex: Copy + Debug;

    fn get(&self, index: &Self::StorageIndex) -> Option<&StorageData>;

    fn get_dependents(
        &self,
        index: &Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex>;

    fn has_dependencies(&self, index: &Self::StorageIndex) -> bool;

    fn remove(&mut self, index: &Self::StorageIndex) -> Option<StorageData>;
}

pub type RatioTipGas = Ratio<u64>;

#[cfg(feature = "u32-tx-count")]
type TxCount = u32;
#[cfg(not(feature = "u32-tx-count"))]
type TxCount = u16;

/// Key used to sort transactions by tip/gas ratio.
/// It first compares the tip/gas ratio, then the creation instant and finally the transaction id.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub struct Key {
    ratio: RatioTipGas,
    creation_instant: SystemTime,
    tx_id: TxId,
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        let cmp = self.ratio.cmp(&other.ratio);
        if cmp == Ordering::Equal {
            let instant_cmp = other.creation_instant.cmp(&self.creation_instant);
            if instant_cmp == Ordering::Equal {
                self.tx_id.cmp(&other.tx_id)
            } else {
                instant_cmp
            }
        } else {
            cmp
        }
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
struct SkipCounters {
    not_enough_gas: usize,
    too_big_tx: usize,
    less_price: usize,
    excluded_contracts: usize,
}

struct SelectionBudget {
    gas_left: u64,
    space_left: u64,
    nb_left: TxCount,
}

impl SelectionBudget {
    fn new(constraints: &Constraints) -> Self {
        Self {
            gas_left: constraints.max_gas,
            space_left: constraints.maximum_block_size,
            nb_left: constraints.maximum_txs,
        }
    }

    fn has_capacity(&self) -> bool {
        self.gas_left > 0 && self.space_left > 0 && self.nb_left > 0
    }
}

/// The selection algorithm that selects transactions based on the tip/gas ratio.
pub struct RatioTipGasSelection<S>
where
    S: RatioTipGasSelectionAlgorithmStorage,
{
    executable_transactions_sorted_tip_gas_ratio: BTreeMap<Reverse<Key>, S::StorageIndex>,
    new_executable_txs_notifier: tokio::sync::watch::Sender<()>,
    eagerly_include_tx_dependency_graphs: bool,
}

impl<S> RatioTipGasSelection<S>
where
    S: RatioTipGasSelectionAlgorithmStorage,
{
    pub fn new(
        new_executable_txs_notifier: tokio::sync::watch::Sender<()>,
        eagerly_include_tx_dependency_graphs: bool,
    ) -> Self {
        Self {
            executable_transactions_sorted_tip_gas_ratio: BTreeMap::new(),
            new_executable_txs_notifier,
            eagerly_include_tx_dependency_graphs,
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.executable_transactions_sorted_tip_gas_ratio.is_empty()
    }

    fn key(store_entry: &StorageData) -> Key {
        let transaction = &store_entry.transaction;
        let tip_gas_ratio =
            RatioTipGas::new(transaction.tip().saturating_add(1), transaction.max_gas());

        Key {
            ratio: tip_gas_ratio,
            creation_instant: store_entry.creation_instant,
            tx_id: transaction.id(),
        }
    }

    fn on_removed_transaction_inner(&mut self, key: Key) {
        self.executable_transactions_sorted_tip_gas_ratio
            .remove(&Reverse(key));
    }

    fn batch_graphs_count(
        executable_count: usize,
        execution_worker_count: usize,
    ) -> usize {
        if execution_worker_count == 0 {
            return 0;
        }
        (executable_count
            .saturating_add(execution_worker_count)
            .saturating_sub(1))
            / execution_worker_count
    }

    fn try_enqueue_stored_transaction(
        &self,
        constraints: &Constraints,
        stored_transaction: &StorageData,
        storage_id: S::StorageIndex,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
    ) -> bool {
        if !budget.has_capacity() {
            return false;
        }

        let has_excluded_contract =
            stored_transaction.transaction.inputs().iter().any(|input| {
                if let fuel_core_types::fuel_tx::Input::Contract(contract) = input {
                    constraints
                        .excluded_contracts
                        .contains(&contract.contract_id)
                } else {
                    false
                }
            });
        if has_excluded_contract {
            skipped.excluded_contracts = skipped.excluded_contracts.saturating_add(1);
            return false;
        }

        if stored_transaction.transaction.max_gas_price() < constraints.minimal_gas_price
        {
            skipped.less_price = skipped.less_price.saturating_add(1);
            return false;
        }

        if stored_transaction.transaction.max_gas() > budget.gas_left {
            skipped.not_enough_gas = skipped.not_enough_gas.saturating_add(1);
            return false;
        }

        if stored_transaction.transaction.metered_bytes_size() as u64 > budget.space_left
        {
            skipped.too_big_tx = skipped.too_big_tx.saturating_add(1);
            return false;
        }

        budget.gas_left = budget
            .gas_left
            .saturating_sub(stored_transaction.transaction.max_gas());
        budget.space_left = budget
            .space_left
            .saturating_sub(stored_transaction.transaction.metered_bytes_size() as u64);
        budget.nb_left = budget.nb_left.saturating_sub(1);
        prioritized_queue.push_back(storage_id);
        true
    }

    fn fill_from_executable_set(
        &mut self,
        constraints: &Constraints,
        storage: &S,
        batch_graphs_count: usize,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
    ) -> bool {
        let mut selected_keys = Vec::new();
        for (key, storage_id) in &self.executable_transactions_sorted_tip_gas_ratio {
            if prioritized_queue.len() >= batch_graphs_count || !budget.has_capacity() {
                break;
            }

            let Some(stored_transaction) = storage.get(storage_id) else {
                debug_assert!(
                    false,
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                tracing::warn!(
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                selected_keys.push(key.0);
                continue;
            };

            if self.try_enqueue_stored_transaction(
                constraints,
                stored_transaction,
                *storage_id,
                prioritized_queue,
                budget,
                skipped,
            ) {
                selected_keys.push(key.0);
            }
        }

        if selected_keys.is_empty() {
            return false;
        }

        for key in selected_keys {
            self.on_removed_transaction_inner(key);
        }
        true
    }

    fn fill_from_promoted_queue(
        &mut self,
        constraints: &Constraints,
        storage: &S,
        batch_graphs_count: usize,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        promoted_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
    ) -> bool {
        let mut filled = false;
        while budget.has_capacity() && prioritized_queue.len() < batch_graphs_count {
            let Some(storage_id) = promoted_queue.pop_front() else {
                break;
            };

            let Some(stored_transaction) = storage.get(&storage_id) else {
                debug_assert!(
                    false,
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                tracing::warn!(
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                continue;
            };

            if self.try_enqueue_stored_transaction(
                constraints,
                stored_transaction,
                storage_id,
                prioritized_queue,
                budget,
                skipped,
            ) {
                let key = Self::key(stored_transaction);
                self.on_removed_transaction_inner(key);
                filled = true;
            }
        }
        filled
    }

    #[cfg(test)]
    pub(crate) fn assert_integrity(&self, expected_txs: &[ArcPoolTx]) {
        let mut expected_txs: HashMap<TxId, ArcPoolTx> = expected_txs
            .iter()
            .map(|tx| (tx.id(), tx.clone()))
            .collect();
        for key in self.executable_transactions_sorted_tip_gas_ratio.keys() {
            expected_txs.remove(&key.0.tx_id).unwrap_or_else(|| {
                panic!(
                    "Transaction with id {:?} is not in the expected transactions.",
                    key.0.tx_id
                )
            });
        }
        assert!(
            expected_txs.is_empty(),
            "Some transactions are missing from the selection algorithm: {:?}",
            expected_txs.keys().collect::<Vec<_>>()
        );
    }
}

impl<S> SelectionAlgorithm for RatioTipGasSelection<S>
where
    S: RatioTipGasSelectionAlgorithmStorage,
{
    type Storage = S;
    type StorageIndex = S::StorageIndex;

    fn gather_best_txs(
        &mut self,
        constraints: &Constraints,
        storage: &mut S,
    ) -> RemovedTransactions {
        let mut result = Vec::new();
        let execution_worker_count = constraints.execution_worker_count.max(1);
        let batch_graphs_count = Self::batch_graphs_count(
            self.number_of_executable_transactions(),
            execution_worker_count,
        );

        let mut budget = SelectionBudget::new(constraints);
        let mut skipped = SkipCounters::default();
        let mut add_new_executable = false;
        let mut prioritized_queue = VecDeque::with_capacity(batch_graphs_count);
        let mut promoted_queue = VecDeque::new();

        self.fill_from_executable_set(
            constraints,
            storage,
            batch_graphs_count,
            &mut prioritized_queue,
            &mut budget,
            &mut skipped,
        );

        loop {
            if budget.has_capacity() {
                let filled_from_promoted = self.fill_from_promoted_queue(
                    constraints,
                    storage,
                    batch_graphs_count,
                    &mut prioritized_queue,
                    &mut promoted_queue,
                    &mut budget,
                    &mut skipped,
                );
                let filled_from_executable = self.eagerly_include_tx_dependency_graphs
                    && prioritized_queue.len() < batch_graphs_count
                    && self.fill_from_executable_set(
                        constraints,
                        storage,
                        batch_graphs_count,
                        &mut prioritized_queue,
                        &mut budget,
                        &mut skipped,
                    );

                if prioritized_queue.is_empty()
                    && !filled_from_promoted
                    && !filled_from_executable
                {
                    break;
                }
            }

            if prioritized_queue.is_empty() {
                break;
            }

            let storage_id = prioritized_queue
                .pop_front()
                .expect("Checked for emptiness above.");

            let Some(_stored_transaction) = storage.get(&storage_id) else {
                debug_assert!(
                    false,
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                tracing::warn!(
                    "Transaction not found in the storage during `gather_best_txs`."
                );
                continue;
            };

            let dependents = storage.get_dependents(&storage_id).collect::<Vec<_>>();
            debug_assert!(!storage.has_dependencies(&storage_id));
            let removed = storage.remove(&storage_id).expect(
                "We just get the transaction from the storage above, it should exist.",
            );
            result.push(removed);

            for dependent in dependents {
                if storage.has_dependencies(&dependent) {
                    continue;
                }
                let Some(store_entry) = storage.get(&dependent) else {
                    debug_assert!(
                        false,
                        "Dependent transaction not found in the storage during `gather_best_txs`."
                    );
                    tracing::warn!(
                        "Dependent transaction not found in the storage during `gather_best_txs`."
                    );
                    continue;
                };
                self.new_executable_transaction(dependent, store_entry);
                promoted_queue.push_back(dependent);
                add_new_executable = true;
            }
        }

        if add_new_executable {
            self.new_executable_txs_notifier.send_replace(());
        }

        tracing::warn!(
            batch_graphs_count,
            execution_worker_count,
            prioritized_queue_len = prioritized_queue.len(),
            promoted_queue_len = promoted_queue.len(),
            selected_count = result.len(),
            skipped_not_enough_gas = skipped.not_enough_gas,
            skipped_too_big_tx = skipped.too_big_tx,
            skipped_less_price = skipped.less_price,
            skipped_excluded_contracts = skipped.excluded_contracts,
            "txpool_v2 gather_best_txs summary"
        );

        result
    }

    fn new_executable_transaction(
        &mut self,
        storage_id: Self::StorageIndex,
        store_entry: &StorageData,
    ) {
        let key = Self::key(store_entry);
        self.executable_transactions_sorted_tip_gas_ratio
            .insert(Reverse(key), storage_id);
        // tracing::warn!(
        //     executable_set_size = self
        //         .executable_transactions_sorted_tip_gas_ratio
        //         .len(),
        //     "txpool_v2 executable added"
        // );
    }

    fn get_less_worth_txs(&self) -> impl Iterator<Item = &Self::StorageIndex> {
        self.executable_transactions_sorted_tip_gas_ratio
            .values()
            .rev()
    }

    fn on_removed_transaction(&mut self, storage_entry: &StorageData) {
        let key = Self::key(storage_entry);
        self.on_removed_transaction_inner(key);
        // tracing::warn!(
        //     executable_set_size = self
        //         .executable_transactions_sorted_tip_gas_ratio
        //         .len(),
        //     "txpool_v2 executable removed"
        // );
    }

    fn number_of_executable_transactions(&self) -> usize {
        self.executable_transactions_sorted_tip_gas_ratio.len()
    }
}
