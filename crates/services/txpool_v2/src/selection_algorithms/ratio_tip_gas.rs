use std::{
    cmp::{
        Ordering,
        Reverse,
    },
    collections::{
        BTreeMap,
        HashMap,
        HashSet,
        VecDeque,
    },
    fmt::Debug,
    time::SystemTime,
};

use fuel_core_types::fuel_tx::{
    ContractId,
    Input,
    TxId,
};
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

struct AnchorState {
    threshold_pct: u8,
    total_gas: u64,
    current_anchor: Option<ContractId>,
    current_anchor_gas: u64,
    locked_anchor: Option<ContractId>,
}

impl AnchorState {
    fn new(threshold_pct: u8) -> Self {
        Self {
            threshold_pct,
            total_gas: 0,
            current_anchor: None,
            current_anchor_gas: 0,
            locked_anchor: None,
        }
    }

    fn on_selected(&mut self, tx_anchor: Option<ContractId>, tx_gas: u64) {
        self.total_gas = self.total_gas.saturating_add(tx_gas);
        if self.locked_anchor.is_some() {
            return;
        }
        if self.current_anchor == tx_anchor {
            self.current_anchor_gas = self.current_anchor_gas.saturating_add(tx_gas);
        } else {
            self.current_anchor = tx_anchor;
            self.current_anchor_gas = tx_gas;
        }
        if self.current_anchor_gas.saturating_mul(100)
            >= self
                .total_gas
                .saturating_mul(u64::from(self.threshold_pct))
        {
            self.locked_anchor = self.current_anchor;
        }
    }
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
    executable_transactions_by_contract:
        HashMap<ContractId, BTreeMap<Reverse<Key>, S::StorageIndex>>,
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
            executable_transactions_by_contract: HashMap::new(),
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

    fn remove_key_from_indexes(
        &mut self,
        key: Key,
        contract_ids: &[ContractId],
    ) {
        self.executable_transactions_sorted_tip_gas_ratio
            .remove(&Reverse(key));
        contract_ids
            .iter()
            .for_each(|contract_id| self.remove_key_from_contract_index(*contract_id, key));
    }

    fn remove_key_from_contract_index(
        &mut self,
        contract_id: ContractId,
        key: Key,
    ) {
        if let Some(contract_index) =
            self.executable_transactions_by_contract.get_mut(&contract_id)
        {
            contract_index.remove(&Reverse(key));
            if contract_index.is_empty() {
                self.executable_transactions_by_contract.remove(&contract_id);
            }
        }
    }

    fn insert_key_into_indexes(
        &mut self,
        key: Key,
        storage_id: S::StorageIndex,
        contract_ids: &[ContractId],
    ) {
        self.executable_transactions_sorted_tip_gas_ratio
            .insert(Reverse(key), storage_id);
        contract_ids.iter().for_each(|contract_id| {
            self.insert_key_into_contract_index(*contract_id, key, storage_id)
        });
    }

    fn insert_key_into_contract_index(
        &mut self,
        contract_id: ContractId,
        key: Key,
        storage_id: S::StorageIndex,
    ) {
        self.executable_transactions_by_contract
            .entry(contract_id)
            .or_default()
            .insert(Reverse(key), storage_id);
    }

    fn collect_contract_inputs(store_entry: &StorageData) -> Vec<ContractId> {
        let mut contracts = HashSet::new();
        store_entry.transaction.inputs().iter().for_each(|input| {
            if let Input::Contract(contract_input) = input {
                contracts.insert(contract_input.contract_id);
            }
        });
        contracts.into_iter().collect()
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
    ) -> Option<u64> {
        if !budget.has_capacity() {
            return None;
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
            return None;
        }

        if stored_transaction.transaction.max_gas_price() < constraints.minimal_gas_price
        {
            skipped.less_price = skipped.less_price.saturating_add(1);
            return None;
        }

        let tx_gas = stored_transaction.transaction.max_gas();
        if tx_gas > budget.gas_left {
            skipped.not_enough_gas = skipped.not_enough_gas.saturating_add(1);
            return None;
        }

        if stored_transaction.transaction.metered_bytes_size() as u64 > budget.space_left
        {
            skipped.too_big_tx = skipped.too_big_tx.saturating_add(1);
            return None;
        }

        budget.gas_left = budget.gas_left.saturating_sub(tx_gas);
        budget.space_left = budget
            .space_left
            .saturating_sub(stored_transaction.transaction.metered_bytes_size() as u64);
        budget.nb_left = budget.nb_left.saturating_sub(1);
        prioritized_queue.push_back(storage_id);
        Some(tx_gas)
    }

    fn fill_from_executable_set(
        &mut self,
        constraints: &Constraints,
        storage: &S,
        queue_limit: usize,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
        anchor_state: Option<&mut AnchorState>,
    ) -> bool {
        let mut anchor_state = anchor_state;
        let mut selected_transactions = Vec::new();
        for (key, storage_id) in &self.executable_transactions_sorted_tip_gas_ratio {
            if prioritized_queue.len() >= queue_limit || !budget.has_capacity() {
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
                selected_transactions.push((key.0, Vec::new()));
                continue;
            };

            if let Some(gas) = self.try_enqueue_stored_transaction(
                constraints,
                stored_transaction,
                *storage_id,
                prioritized_queue,
                budget,
                skipped,
            ) {
                if let Some(state) = anchor_state.as_mut() {
                    state.on_selected(
                        self.select_anchor_contract(constraints, stored_transaction),
                        gas,
                    );
                }
                selected_transactions
                    .push((key.0, Self::collect_contract_inputs(stored_transaction)));
            }
        }

        if selected_transactions.is_empty() {
            return false;
        }

        selected_transactions
            .into_iter()
            .for_each(|(key, contracts)| self.remove_key_from_indexes(key, &contracts));
        true
    }

    fn fill_from_anchor_contract_set(
        &mut self,
        constraints: &Constraints,
        storage: &S,
        queue_limit: usize,
        anchor_contract_id: ContractId,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
        anchor_state: Option<&mut AnchorState>,
    ) -> bool {
        let mut anchor_state = anchor_state;
        let candidates = self
            .executable_transactions_by_contract
            .get(&anchor_contract_id)
            .map(|entries| {
                entries
                    .iter()
                    .map(|(key, storage_id)| (key.0, *storage_id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if candidates.is_empty() {
            return false;
        }

        let mut selected_transactions = Vec::new();
        let mut stale_keys = Vec::new();
        for (key, storage_id) in candidates {
            if prioritized_queue.len() >= queue_limit || !budget.has_capacity() {
                break;
            }

            let Some(stored_transaction) = storage.get(&storage_id) else {
                stale_keys.push(key);
                continue;
            };

            if let Some(gas) = self.try_enqueue_stored_transaction(
                constraints,
                stored_transaction,
                storage_id,
                prioritized_queue,
                budget,
                skipped,
            ) {
                if let Some(state) = anchor_state.as_mut() {
                    state.on_selected(Some(anchor_contract_id), gas);
                }
                selected_transactions
                    .push((key, Self::collect_contract_inputs(stored_transaction)));
            }
        }

        if selected_transactions.is_empty() && stale_keys.is_empty() {
            return false;
        }

        stale_keys.into_iter().for_each(|key| {
            self.executable_transactions_sorted_tip_gas_ratio
                .remove(&Reverse(key));
            self.remove_key_from_contract_index(anchor_contract_id, key);
        });
        selected_transactions
            .into_iter()
            .for_each(|(key, contracts)| self.remove_key_from_indexes(key, &contracts));
        true
    }

    fn fill_from_promoted_queue(
        &mut self,
        constraints: &Constraints,
        storage: &S,
        queue_limit: usize,
        prioritized_queue: &mut VecDeque<S::StorageIndex>,
        promoted_queue: &mut VecDeque<S::StorageIndex>,
        budget: &mut SelectionBudget,
        skipped: &mut SkipCounters,
        anchor_state: Option<&mut AnchorState>,
    ) -> bool {
        let mut anchor_state = anchor_state;
        let mut filled = false;
        while budget.has_capacity() && prioritized_queue.len() < queue_limit {
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

            if let Some(gas) = self.try_enqueue_stored_transaction(
                constraints,
                stored_transaction,
                storage_id,
                prioritized_queue,
                budget,
                skipped,
            ) {
                if let Some(state) = anchor_state.as_mut() {
                    state.on_selected(
                        self.select_anchor_contract(constraints, stored_transaction),
                        gas,
                    );
                }
                let key = Self::key(stored_transaction);
                self.remove_key_from_indexes(
                    key,
                    &Self::collect_contract_inputs(stored_transaction),
                );
                filled = true;
            }
        }
        filled
    }

    fn contract_anchor_bias_enabled() -> bool {
        std::env::var("TXPOOL_V2_CONTRACT_ANCHOR_BIAS")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    fn contract_anchor_dominance_threshold_pct() -> u8 {
        std::env::var("TXPOOL_V2_CONTRACT_ANCHOR_DOMINANCE_THRESHOLD_PCT")
            .ok()
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|value| *value > 0 && *value <= 100)
            .unwrap_or(50)
    }

    fn select_anchor_contract(
        &self,
        constraints: &Constraints,
        store_entry: &StorageData,
    ) -> Option<ContractId> {
        Self::collect_contract_inputs(store_entry)
            .into_iter()
            .filter(|contract_id| !constraints.excluded_contracts.contains(contract_id))
            .max_by_key(|contract_id| {
                self.executable_transactions_by_contract
                    .get(contract_id)
                    .map(BTreeMap::len)
                    .unwrap_or(0)
            })
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
        let contract_anchor_bias_enabled = Self::contract_anchor_bias_enabled();
        let contract_anchor_dominance_threshold_pct =
            Self::contract_anchor_dominance_threshold_pct();
        let mut anchor_state = contract_anchor_bias_enabled
            .then(|| AnchorState::new(contract_anchor_dominance_threshold_pct));
        let initial_queue_limit = if contract_anchor_bias_enabled {
            1
        } else {
            batch_graphs_count
        };

        self.fill_from_executable_set(
            constraints,
            storage,
            initial_queue_limit,
            &mut prioritized_queue,
            &mut budget,
            &mut skipped,
            anchor_state.as_mut(),
        );
        let mut anchor_contract_id = if contract_anchor_bias_enabled {
            prioritized_queue
                .front()
                .and_then(|storage_id| storage.get(storage_id))
                .and_then(|store_entry| {
                    self.select_anchor_contract(constraints, store_entry)
                })
        } else {
            None
        };

        loop {
            if budget.has_capacity() {
                let locked_anchor = anchor_state
                    .as_ref()
                    .and_then(|state| state.locked_anchor);
                if let Some(locked_anchor) = locked_anchor {
                    let filled_from_locked_anchor = self.fill_from_anchor_contract_set(
                        constraints,
                        storage,
                        batch_graphs_count,
                        locked_anchor,
                        &mut prioritized_queue,
                        &mut budget,
                        &mut skipped,
                        anchor_state.as_mut(),
                    );
                    if !filled_from_locked_anchor && prioritized_queue.is_empty() {
                        break;
                    }
                }
                let filled_from_promoted = self.fill_from_promoted_queue(
                    constraints,
                    storage,
                    batch_graphs_count,
                    &mut prioritized_queue,
                    &mut promoted_queue,
                    &mut budget,
                    &mut skipped,
                    anchor_state.as_mut(),
                );
                let filled_from_anchor = anchor_contract_id
                    .filter(|_| prioritized_queue.len() < batch_graphs_count)
                    .map(|anchor_contract| {
                        self.fill_from_anchor_contract_set(
                            constraints,
                            storage,
                            batch_graphs_count,
                            anchor_contract,
                            &mut prioritized_queue,
                            &mut budget,
                            &mut skipped,
                            anchor_state.as_mut(),
                        )
                    })
                    .unwrap_or(false);
                let should_fill_from_executable =
                    prioritized_queue.len() < batch_graphs_count
                        && self.eagerly_include_tx_dependency_graphs;
                let filled_from_executable = should_fill_from_executable
                    && self.fill_from_executable_set(
                        constraints,
                        storage,
                        batch_graphs_count,
                        &mut prioritized_queue,
                        &mut budget,
                        &mut skipped,
                        anchor_state.as_mut(),
                    );

                if prioritized_queue.is_empty()
                    && !filled_from_promoted
                    && !filled_from_anchor
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
                if anchor_contract_id.is_none() {
                    anchor_contract_id =
                        self.select_anchor_contract(constraints, store_entry);
                }
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
            contract_anchor_bias_enabled,
            contract_anchor_dominance_threshold_pct,
            anchor_contract = ?anchor_contract_id,
            locked_anchor = ?anchor_state.as_ref().and_then(|state| state.locked_anchor),
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
        self.insert_key_into_indexes(
            key,
            storage_id,
            &Self::collect_contract_inputs(store_entry),
        );
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
        self.remove_key_from_indexes(
            key,
            &Self::collect_contract_inputs(storage_entry),
        );
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
