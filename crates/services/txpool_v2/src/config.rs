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

use crate::error::BlacklistedError;

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
    pub fn check_blacklisting(
        &self,
        tx: &PoolTransaction,
    ) -> Result<(), BlacklistedError> {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, owner, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, owner, .. }) => {
                    if self.coins.contains(utxo_id) {
                        return Err(BlacklistedError::BlacklistedUTXO(*utxo_id));
                    }
                    if self.owners.contains(owner) {
                        return Err(BlacklistedError::BlacklistedOwner(*owner));
                    }
                }
                Input::Contract(contract) => {
                    if self.contracts.contains(&contract.contract_id) {
                        return Err(BlacklistedError::BlacklistedContract(
                            contract.contract_id,
                        ));
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
                        return Err(BlacklistedError::BlacklistedMessage(*nonce));
                    }
                    if self.owners.contains(sender) {
                        return Err(BlacklistedError::BlacklistedOwner(*sender));
                    }
                    if self.owners.contains(recipient) {
                        return Err(BlacklistedError::BlacklistedOwner(*recipient));
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Enable UTXO validation (will check if UTXO exists in the database and has correct data).
    pub utxo_validation: bool,
    /// Maximum of subscriptions to listen to updates of a transaction.
    pub max_tx_update_subscriptions: usize,
    /// Maximum transactions per dependencies chain.
    pub max_txs_chain_count: usize,
    /// Pool limits
    pub pool_limits: PoolLimits,
    /// Service channel limits
    pub service_channel_limits: ServiceChannelLimits,
    /// Interval for checking the time to live of transactions.
    pub ttl_check_interval: Duration,
    /// Maximum transaction time to live.
    pub max_txs_ttl: Duration,
    /// Heavy async processing configuration.
    pub heavy_work: HeavyWorkConfig,
    /// Blacklist. Transactions with blacklisted inputs will not be accepted.
    pub black_list: BlackList,
}

#[derive(Clone, Debug)]
pub struct PoolLimits {
    /// Maximum number of transactions in the pool.
    pub max_txs: usize,
    /// Maximum number of gas in the pool.
    pub max_gas: u64,
    /// Maximum number of bytes in the pool.
    pub max_bytes_size: usize,
}

#[derive(Clone, Debug)]
pub struct ServiceChannelLimits {
    /// Maximum number of pending requests waiting in the write pool channel.
    pub max_pending_write_pool_requests: usize,
    /// Maximum number of pending requests waiting in the read pool channel.
    pub max_pending_read_pool_requests: usize,
}

#[derive(Clone, Debug)]
pub struct HeavyWorkConfig {
    /// Maximum of threads for managing verifications/insertions.
    pub number_threads_to_verify_transactions: usize,
    /// Maximum of tasks in the heavy async processing queue.
    pub size_of_verification_queue: usize,
    /// Maximum number of threads for managing the p2p synchronisation
    pub number_threads_p2p_sync: usize,
    /// Maximum number of tasks in the p2p sync queue
    pub size_of_p2p_sync_queue: usize,
}

#[cfg(feature = "test-helpers")]
impl Default for Config {
    fn default() -> Self {
        Self {
            utxo_validation: true,
            max_tx_update_subscriptions: 1000,
            max_txs_chain_count: 50,
            ttl_check_interval: Duration::from_secs(60),
            max_txs_ttl: Duration::from_secs(60 * 10),
            black_list: BlackList::default(),
            pool_limits: PoolLimits {
                max_txs: 10000,
                max_gas: 100_000_000_000,
                max_bytes_size: 1_000_000_000,
            },
            heavy_work: HeavyWorkConfig {
                // It is important for tests to have only one thread for verification
                // because some of them rely on the ordering of insertion.
                number_threads_to_verify_transactions: 1,
                size_of_verification_queue: 100,
                number_threads_p2p_sync: 1,
                size_of_p2p_sync_queue: 100,
            },
            service_channel_limits: ServiceChannelLimits {
                max_pending_write_pool_requests: 1000,
                max_pending_read_pool_requests: 1000,
            },
        }
    }
}
