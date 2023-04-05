use fuel_core_types::{
    fuel_types::Word,
    services::txpool::ArcPoolTx,
};
use std::cmp::Reverse;

// transaction selection could use a plugin based approach in the
// future for block producers to customize block building (e.g. alternative priorities besides gas fees)

pub fn select_transactions(
    mut includable_txs: Vec<ArcPoolTx>,
    max_gas: u64,
) -> Vec<ArcPoolTx> {
    // Select all txs that fit into the block, preferring ones with higher gas price.
    //
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.
    let mut used_block_space: Word = 0;

    // Sort transactions by gas price, highest first
    includable_txs.sort_by_key(|a| Reverse(a.price()));

    // Pick as many transactions as we can fit into the block (greedy)
    includable_txs
        .into_iter()
        .filter(|tx| {
            let tx_block_space = tx.max_gas();
            if let Some(new_used_space) = used_block_space.checked_add(tx_block_space) {
                if new_used_space <= max_gas {
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
    use fuel_core_txpool as _;
    use fuel_core_types::{
        fuel_asm::{
            op,
            RegId,
        },
        fuel_crypto::rand::{
            thread_rng,
            Rng,
        },
        fuel_tx::{
            ConsensusParameters,
            Output,
            TransactionBuilder,
        },
        fuel_vm::checked_transaction::builder::TransactionBuilderExt,
    };
    use itertools::Itertools;
    use std::sync::Arc;

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
                    vec![op::ret(RegId::ONE)].into_iter().collect(),
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
                    Default::default(),
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                // The block producer assumes transactions are already checked
                // so it doesn't need to compute valid sigs for tests
                .finalize_checked_basic(
                    Default::default(),
                    &ConsensusParameters {
                        gas_price_factor: 1,
                        ..ConsensusParameters::default()
                    },
                ).into()
            })
            .map(Arc::new)
            .collect();

        select_transactions(txs, block_gas_limit)
            .into_iter()
            .map(|tx| TxGas {
                limit: tx.limit(),
                price: tx.price(),
            })
            .collect()
    }

    #[test]
    fn selector_works_with_empty_input() {
        let selected = make_txs_and_select(&[], 1_000_000);
        assert!(selected.is_empty());
    }

    #[rstest::rstest]
    #[test]
    #[case(1000, vec![])]
    #[case(2500, vec![TxGas { price: 5, limit: 1000 }])]
    #[case(5000, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 2, limit: 1000 }])
    ]
    #[case(7500, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 4, limit: 3000 }
    ])]
    #[case(10_000, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 4, limit: 3000 },
        TxGas { price: 2, limit: 1000 }
    ])]
    #[case(12_500, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 4, limit: 3000 },
        TxGas { price: 3, limit: 2000 }
    ])]
    #[case(15_000, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 4, limit: 3000 },
        TxGas { price: 3, limit: 2000 },
        TxGas { price: 2, limit: 1000 }
    ])]
    #[case(17_500, vec![
        TxGas { price: 5, limit: 1000 },
        TxGas { price: 4, limit: 3000 },
        TxGas { price: 3, limit: 2000 },
        TxGas { price: 2, limit: 1000 },
        TxGas { price: 1, limit: 1000 }
    ])]
    fn selector_prefers_highest_gas_txs_and_sorts(
        #[case] selection_limit: u64,
        #[case] expected_txs: Vec<TxGas>,
    ) {
        #[rustfmt::skip]
        let original = [
            TxGas { price: 3, limit: 2000 },
            TxGas { price: 1, limit: 1000 },
            TxGas { price: 4, limit: 3000 },
            TxGas { price: 5, limit: 1000 },
            TxGas { price: 2, limit: 1000 },
        ];

        let selected = make_txs_and_select(&original, selection_limit);
        assert_eq!(
            expected_txs, selected,
            "Wrong txs selected for max_gas: {selection_limit}"
        );
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
