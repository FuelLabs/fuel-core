//! Clap configuration related to TxPool service.

use fuel_core_types::{
    fuel_tx::{
        Address,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
};

#[derive(Debug, Clone, clap::Args)]
pub struct TxPoolArgs {
    /// The max time to live of the transaction inside of the `TxPool`.
    #[clap(long = "tx-pool-ttl", default_value = "5m", env)]
    pub tx_pool_ttl: humantime::Duration,

    /// The interval for checking the time to live of transactions.
    #[clap(long = "tx-ttl-check-interval", default_value = "1m", env)]
    pub tx_ttl_check_interval: humantime::Duration,

    /// The max number of transactions that the `TxPool` can simultaneously store.
    #[clap(long = "tx-max-number", default_value = "4064", env)]
    pub tx_max_number: usize,

    /// The max number of gas the `TxPool` can simultaneously store.
    #[clap(long = "tx-max-total-gas", default_value = "3_000_000_000", env)]
    pub tx_max_total_gas: u64,

    /// The max number of bytes that the `TxPool` can simultaneously store.
    #[clap(long = "tx-max-total-bytes", default_value = "10_000_000", env)]
    pub tx_max_total_bytes: usize,

    /// The max number of tx in a chain of dependent transactions that supported by the `TxPool`.
    #[clap(long = "tx-max-chain-count", default_value = "10", env)]
    pub tx_max_chain_count: usize,

    /// The maximum number of active subscriptions that supported by the `TxPool`.
    #[clap(long = "tx-number-active-subscriptions", default_value = "4064", env)]
    pub tx_number_active_subscriptions: usize,

    /// The list of banned addresses ignored by the `TxPool`.
    #[clap(long = "tx-blacklist-addresses", value_delimiter = ',', env)]
    pub tx_blacklist_addresses: Vec<Address>,

    /// The list of banned coins ignored by the `TxPool`.
    #[clap(long = "tx-blacklist-coins", value_delimiter = ',', env)]
    pub tx_blacklist_coins: Vec<UtxoId>,

    /// The list of banned messages ignored by the `TxPool`.
    #[clap(long = "tx-blacklist-messages", value_delimiter = ',', env)]
    pub tx_blacklist_messages: Vec<Nonce>,

    /// The list of banned contracts ignored by the `TxPool`.
    #[clap(long = "tx-blacklist-contracts", value_delimiter = ',', env)]
    pub tx_blacklist_contracts: Vec<ContractId>,

    /// Number of threads for managing verifications/insertions.
    #[clap(
        long = "tx-number-threads-to-verify-transactions",
        default_value = "4",
        env
    )]
    pub tx_number_threads_to_verify_transactions: usize,

    /// Maximum number of tasks in the verifications/insertions queue.
    #[clap(long = "tx-size-of-verification-queue", default_value = "200", env)]
    pub tx_size_of_verification_queue: usize,

    /// Number of threads for managing the p2p synchronisation.
    #[clap(long = "tx-number-threads-p2p-sync", default_value = "2", env)]
    pub tx_number_threads_p2p_sync: usize,

    /// Maximum number of tasks in the p2p synchronisation queue.
    #[clap(long = "tx-size-of-p2p-sync-queue", default_value = "20", env)]
    pub tx_size_of_p2p_sync_queue: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use fuel_core::txpool::config::BlackList;
    use test_case::test_case;

    #[derive(Debug, Clone, Parser)]
    pub struct Command {
        #[clap(flatten)]
        tx_pool: TxPoolArgs,
    }

    fn blacklist(
        a: Vec<Address>,
        c: Vec<UtxoId>,
        m: Vec<Nonce>,
        ct: Vec<ContractId>,
    ) -> BlackList {
        BlackList::new(a, c, m, ct)
    }

    #[test_case(&[""] => Ok(blacklist(vec![], vec![], vec![], vec![])); "defaults works")]
    #[test_case(&["", "--tx-blacklist-addresses=\
            0x0000000000000000000000000000000000000000000000000000000000000000,\
            0101010101010101010101010101010101010101010101010101010101010101"
        ]
        => Ok(blacklist(vec![[0; 32].into(), [1; 32].into()], vec![], vec![], vec![])); "addresses works")]
    #[test_case(&["", "--tx-blacklist-coins=\
            0x00000000000000000000000000000000000000000000000000000000000000000002,\
            01010101010101010101010101010101010101010101010101010101010101010003"
    ]
    => Ok(blacklist(vec![], vec![UtxoId::new([0; 32].into(), 2), UtxoId::new([1; 32].into(), 3)], vec![], vec![])); "coins works")]
    #[test_case(&["", "--tx-blacklist-messages=\
            0x0000000000000000000000000000000000000000000000000000000000000000,\
            0101010101010101010101010101010101010101010101010101010101010101"
    ]
    => Ok(blacklist(vec![], vec![], vec![[0; 32].into(), [1; 32].into()], vec![])); "messages works")]
    #[test_case(&["", "--tx-blacklist-contracts=\
            0x0000000000000000000000000000000000000000000000000000000000000000,\
            0101010101010101010101010101010101010101010101010101010101010101"
    ]
    => Ok(blacklist(vec![], vec![], vec![], vec![[0; 32].into(), [1; 32].into()])); "contracts works")]
    fn parse(args: &[&str]) -> Result<BlackList, String> {
        let command: Command =
            Command::try_parse_from(args).map_err(|e| e.to_string())?;
        let args = command.tx_pool;

        let blacklist = blacklist(
            args.tx_blacklist_addresses,
            args.tx_blacklist_coins,
            args.tx_blacklist_messages,
            args.tx_blacklist_contracts,
        );

        Ok(blacklist)
    }
}
