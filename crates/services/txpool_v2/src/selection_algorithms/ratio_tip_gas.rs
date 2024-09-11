use std::{
    cmp::{
        Ordering,
        Reverse,
    },
    collections::BTreeMap,
    time::Instant,
};

use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::PoolTransaction,
};
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

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub struct Key {
    ratio: RatioTipGas,
    creation_instant: Instant,
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

pub struct RatioTipGasSelection<S: Storage> {
    transactions_sorted_tip_gas_ratio: BTreeMap<Reverse<Key>, S::StorageIndex>,
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
        storage: &S,
    ) -> Result<Vec<S::StorageIndex>, Error> {
        let mut gas_left = constraints.max_gas;
        let mut best_transactions = Vec::new();

        // Take the first transaction with the highest tip/gas ratio if it fits in the gas limit
        // then promote all its dependents to the list of transactions to be executed
        // and repeat the process until the gas limit is reached
        while gas_left > 0 && !self.transactions_sorted_tip_gas_ratio.is_empty() {
            let mut new_executables = vec![];
            let mut best_transaction = None;

            let sorted_iter = self.transactions_sorted_tip_gas_ratio.iter();
            for (key, storage_id) in sorted_iter {
                let enough_gas = {
                    let stored_transaction = storage.get(storage_id)?;
                    stored_transaction.transaction.max_gas() <= gas_left
                };
                if enough_gas {
                    new_executables.extend(storage.get_dependents(*storage_id)?);
                    let stored_tx = storage.get(storage_id)?;
                    gas_left -= stored_tx.transaction.max_gas();
                    best_transaction = Some((*key, *storage_id));
                    break;
                }
            }

            // Promote its dependents
            self.new_executable_transactions(new_executables, storage)?;
            // Remove the best transaction from the sorted list
            if let Some((key, best_transaction)) = best_transaction {
                self.transactions_sorted_tip_gas_ratio.remove(&key);
                best_transactions.push(best_transaction);
            } else {
                // If no transaction fits in the gas limit,
                // we can break the loop
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
            let key = Key {
                ratio: tip_gas_ratio,
                creation_instant: Instant::now(),
                tx_id: stored_transaction.transaction.id(),
            };
            self.transactions_sorted_tip_gas_ratio
                .insert(Reverse(key), storage_id);
        }
        Ok(())
    }

    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error> {
        let tip_gas_ratio = RatioTipGas::new(transaction.tip(), transaction.max_gas());
        let key = Key {
            ratio: tip_gas_ratio,
            creation_instant: Instant::now(),
            tx_id: transaction.id(),
        };
        self.transactions_sorted_tip_gas_ratio.remove(&Reverse(key));
        Ok(())
    }
}
