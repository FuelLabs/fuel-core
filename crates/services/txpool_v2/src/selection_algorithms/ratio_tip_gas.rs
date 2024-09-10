use std::collections::BTreeMap;

use fuel_core_types::services::txpool::PoolTransaction;
use num_rational::Ratio;

use crate::{
    error::Error,
    storage::Storage,
};

use super::{
    Constraints,
    SelectionAlgorithm,
};

pub type RatioTipGas = Ratio<u64>;

pub struct RatioTipGasSelection<S: Storage> {
    transactions_sorted_tip_gas_ratio: BTreeMap<RatioTipGas, S::StorageIndex>,
}

impl<S: Storage> RatioTipGasSelection<S> {
    pub fn new() -> Self {
        Self {
            transactions_sorted_tip_gas_ratio: BTreeMap::new(),
        }
    }
}

impl<S: Storage> Default for RatioTipGasSelection<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Storage> SelectionAlgorithm<S> for RatioTipGasSelection<S> {
    fn gather_best_txs(
        &mut self,
        constraints: Constraints,
        storage: &mut S,
    ) -> Result<Vec<PoolTransaction>, Error> {
        let mut gas_left = constraints.max_gas;
        let mut best_transactions = Vec::new();

        // Take the first transaction with the highest tip/gas ratio if it fits in the gas limit
        // then promote all its dependents to the list of transactions to be executed
        // and repeat the process until the gas limit is reached
        while gas_left > 0 && !self.transactions_sorted_tip_gas_ratio.is_empty() {
            let mut new_executables = vec![];
            let mut all_txs_checked = true;

            let mut sorted_iter = self.transactions_sorted_tip_gas_ratio.iter();
            for (_, storage_id) in sorted_iter {
                let enough_gas = {
                    let stored_transaction = storage.get(storage_id)?;
                    stored_transaction.transaction.max_gas() <= gas_left
                };
                if enough_gas {
                    let stored_tx = storage.remove_transaction(*storage_id)?;
                    gas_left -= stored_tx.transaction.max_gas();
                    best_transactions.push(stored_tx.transaction);
                    new_executables.extend(storage.get_dependents(*storage_id)?);
                    all_txs_checked = false;
                    break;
                }
            }

            self.new_executable_transactions(new_executables, storage)?;
            if all_txs_checked {
                break;
            }
        }
        Ok(best_transactions)
    }

    fn new_executable_transactions(
        &mut self,
        transactions_ids: Vec<S::StorageIndex>,
        storage: &S,
    ) -> Result<(), Error> {
        for storage_id in transactions_ids {
            let stored_transaction = storage.get(&storage_id)?;
            let tip_gas_ratio = RatioTipGas::new(
                stored_transaction.transaction.tip(),
                stored_transaction.transaction.max_gas(),
            );
            self.transactions_sorted_tip_gas_ratio
                .insert(tip_gas_ratio, storage_id);
        }
        Ok(())
    }
}
