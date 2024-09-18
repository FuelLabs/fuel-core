use std::{
    collections::HashSet,
    time::Duration,
};

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
        Address,
        ContractId,
        Input,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::error::Error;

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct BlackList {
    /// Blacklisted addresses.
    pub owners: HashSet<Address>,
    /// Blacklisted UTXO ids.
    pub coins: HashSet<UtxoId>,
    /// Blacklisted messages by `Nonce`.
    pub messages: HashSet<Nonce>,
    /// Blacklisted contracts.
    pub contracts: HashSet<ContractId>,
}

impl BlackList {
    /// Create a new blacklist.
    pub fn new(
        owners: Vec<Address>,
        utxo_ids: Vec<UtxoId>,
        messages: Vec<Nonce>,
        contracts: Vec<ContractId>,
    ) -> Self {
        Self {
            owners: owners.into_iter().collect(),
            coins: utxo_ids.into_iter().collect(),
            messages: messages.into_iter().collect(),
            contracts: contracts.into_iter().collect(),
        }
    }

    /// Check if the transaction has blacklisted inputs.
    pub fn check_blacklisting(&self, tx: &PoolTransaction) -> Result<(), Error> {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, owner, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, owner, .. }) => {
                    if self.coins.contains(utxo_id) {
                        return Err(Error::BlacklistedUTXO(*utxo_id));
                    }
                    if self.owners.contains(owner) {
                        return Err(Error::BlacklistedOwner(*owner));
                    }
                }
                Input::Contract(contract) => {
                    if self.contracts.contains(&contract.contract_id) {
                        return Err(Error::BlacklistedContract(contract.contract_id));
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageCoinPredicate(MessageCoinPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataSigned(MessageDataSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataPredicate(MessageDataPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                }) => {
                    if self.messages.contains(nonce) {
                        return Err(Error::BlacklistedMessage(*nonce));
                    }
                    if self.owners.contains(sender) {
                        return Err(Error::BlacklistedOwner(*sender));
                    }
                    if self.owners.contains(recipient) {
                        return Err(Error::BlacklistedOwner(*recipient));
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct Config {
    /// Enable UTXO validation (will check if UTXO exists in the database and has correct data).
    pub utxo_validation: bool,
    /// Maximum block size in bytes.
    pub max_block_size: u64,
    /// Maximum gas per block.
    pub max_block_gas: u64,
    /// Maximum transactions per dependencies chain.
    pub max_dependent_txn_count: u64,
    /// Maximum transactions in the pool.
    pub max_txs: u64,
    /// Maximum transaction time to live.
    pub max_txs_ttl: Duration,
    /// Heavy async processing configuration.
    pub heavy_work: HeavyWorkConfig,
    /// Blacklist. Transactions with blacklisted inputs will not be accepted.
    pub black_list: BlackList,
}

#[derive(Clone)]
pub struct HeavyWorkConfig {
    /// Maximum of threads for managing verifications/insertions.
    pub number_threads_verif_insert_transactions: usize,
    /// Maximum of tasks in the heavy async processing queue.
    pub number_pending_tasks_threads_verif_insert_transactions: usize,
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            utxo_validation: true,
            max_block_gas: 100000000,
            max_block_size: 1000000000,
            max_dependent_txn_count: 1000,
            max_txs: 10000,
            max_txs_ttl: Duration::from_secs(60 * 10),
            black_list: BlackList::default(),
            heavy_work: HeavyWorkConfig {
                number_threads_verif_insert_transactions: 4,
                number_pending_tasks_threads_verif_insert_transactions: 100,
            },
        }
    }
}
