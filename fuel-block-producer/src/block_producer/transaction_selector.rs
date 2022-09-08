use std::cmp::Reverse;

use crate::Config;
use fuel_core_interfaces::common::{
    fuel_tx::CheckedTransaction,
    fuel_types::Word,
};

pub fn select_transactions(
    mut includable_txs: Vec<CheckedTransaction>,
    config: &Config,
) -> Vec<CheckedTransaction> {
    // Select all txs that fit into the block, preferring ones with higher gas price.
    //
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.
    let mut used_block_space: Word = 0;

    // Sort transactions by gas price, highest first
    includable_txs.sort_by_key(|a| Reverse(a.transaction().gas_price()));

    // Pick as many transactions as we can fit into the block (greedy)
    includable_txs
        .into_iter()
        .filter(|tx| {
            let tx_block_space = tx.max_fee();
            if let Some(new_used_space) = used_block_space.checked_add(tx_block_space) {
                if new_used_space <= config.max_gas_per_block {
                    used_block_space = new_used_space;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use fuel_core_interfaces::common::{
        fuel_asm::Opcode,
        fuel_crypto::rand::{
            thread_rng,
            Rng,
        },
        fuel_tx::{
            ConsensusParameters,
            Output,
            TransactionBuilder,
        },
        fuel_vm::consts::REG_ONE,
    };
    use itertools::Itertools;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct TxGas {
        pub price: u64,
        pub limit: u64,
    }

    /// A test helper that generates set of txs with given gas prices and limits and runs
    /// `select_transactions` against that, returning the list of selected gas price, limit pairs
    fn make_txs_and_select(txs: &[TxGas], block_gas_limit: Word) -> Vec<TxGas> {
        let mut rng = thread_rng();

        let txs = txs
            .iter()
            .map(|tx_gas| {
                TransactionBuilder::script(
                    vec![Opcode::RET(REG_ONE)].into_iter().collect(),
                    vec![],
                )
                .gas_price(tx_gas.price)
                .gas_limit(tx_gas.limit)
                .add_unsigned_coin_input(
                    rng.gen(),
                    rng.gen(),
                    1_000_000,
                    Default::default(),
                    Default::default(),
                    0,
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                .finalize_checked(
                    0,
                    &ConsensusParameters {
                        gas_price_factor: 1,
                        ..ConsensusParameters::default()
                    },
                )
            })
            .collect();

        select_transactions(
            txs,
            &Config {
                max_gas_per_block: block_gas_limit,
                ..Default::default()
            },
        )
        .into_iter()
        .map(|tx| {
            let tx = tx.transaction();
            TxGas {
                limit: tx.gas_limit(),
                price: tx.gas_price(),
            }
        })
        .collect()
    }

    #[test]
    fn selector_works_with_empty_input() {
        let selected = make_txs_and_select(&[], 1_000_000);
        assert!(selected.is_empty());
    }

    #[test]
    fn selector_prefers_highest_gas_txs_and_sorts() {
        #[rustfmt::skip]
        let original = [
            TxGas { price: 3, limit: 2000 },
            TxGas { price: 1, limit: 1000 },
            TxGas { price: 4, limit: 3000 },
            TxGas { price: 5, limit: 1000 },
            TxGas { price: 2, limit: 1000 },
        ];

        #[rustfmt::skip]
        let ordered = [
            TxGas { price: 5, limit: 1000 },
            TxGas { price: 4, limit: 3000 },
            TxGas { price: 3, limit: 2000 },
            TxGas { price: 2, limit: 1000 },
            TxGas { price: 1, limit: 1000 },
        ];

        let selected = make_txs_and_select(&original, 1_000_000);
        assert_eq!(ordered, selected.as_slice());
    }

    #[test]
    fn selector_doesnt_exceed_max_gas_per_block() {
        #[rustfmt::skip]
        let original = [
            TxGas { price: 3, limit: 2000 },
            TxGas { price: 1, limit: 1000 },
            TxGas { price: 4, limit: 3000 },
            TxGas { price: 5, limit: 1000 },
            TxGas { price: 2, limit: 1000 },
        ];

        for k in 0..original.len() {
            for perm in original.into_iter().permutations(k) {
                for gas_limit in [999, 1000, 2000, 2500, 3000, 5000, 6000, 10_000] {
                    let selected = make_txs_and_select(perm.as_slice(), gas_limit);
                    let total_gas: Word = selected.iter().map(|g| g.limit).sum();
                    assert!(total_gas <= gas_limit);
                }
            }
        }
    }
}
