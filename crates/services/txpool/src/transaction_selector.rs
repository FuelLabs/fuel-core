use fuel_core_types::{
    fuel_types::Word,
    services::txpool::ArcPoolTx,
};

// transaction selection could use a plugin based approach in the
// future for block producers to customize block building (e.g. alternative priorities besides gas fees)

// Expects sorted by gas price transactions, highest first
pub fn select_transactions(
    includable_txs: impl Iterator<Item = ArcPoolTx>,
    max_gas: u64,
) -> Vec<ArcPoolTx> {
    // Select all txs that fit into the block, preferring ones with higher gas price.
    //
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.
    let mut used_block_space: Word = 0;
    // The type of the index for the transaction is `u16`, so we need to
    // limit it to `MAX` value minus 1(because of the `Mint` transaction).
    let takes_txs = u16::MAX - 1;

    // Pick as many transactions as we can fit into the block (greedy)
    includable_txs
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
        .take(takes_txs as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use fuel_core_txpool as _;
    use fuel_core_types::{
        blockchain::header::ConsensusParametersVersion,
        fuel_asm::{
            op,
            RegId,
        },
        fuel_crypto::rand::{
            thread_rng,
            Rng,
        },
        fuel_tx::{
            FeeParameters,
            GasCosts,
            Output,
            TransactionBuilder,
        },
        fuel_vm::{
            checked_transaction::builder::TransactionBuilderExt,
            SecretKey,
        },
        services::txpool::PoolTransaction,
    };
    use itertools::Itertools;
    use std::sync::Arc;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct TxGas {
        pub tip: u64,
        pub limit: u64,
    }

    /// A test helper that generates set of txs with given gas prices and limits and runs
    /// `select_transactions` against that, returning the list of selected gas price, limit pairs
    fn make_txs_and_select(txs: &[TxGas], block_gas_limit: Word) -> Vec<TxGas> {
        let mut rng = thread_rng();

        let fee_params = FeeParameters::default()
            .with_gas_per_byte(0)
            .with_gas_price_factor(1);

        let mut txs = txs
            .iter()
            .map(|tx_gas| {
                let script = TransactionBuilder::script(
                    vec![op::ret(RegId::ONE)].into_iter().collect(),
                    vec![],
                )
                .tip(tx_gas.tip)
                .script_gas_limit(tx_gas.limit)
                .add_unsigned_coin_input(
                    SecretKey::random(&mut rng),
                    rng.gen(),
                    1_000_000,
                    Default::default(),
                    Default::default(),
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                .with_fee_params(fee_params)
                .with_gas_costs(GasCosts::free())
                // The block producer assumes transactions are already checked
                // so it doesn't need to compute valid sigs for tests
                .finalize_checked_basic(Default::default());

                PoolTransaction::Script(script, ConsensusParametersVersion::MIN)
            })
            .map(Arc::new)
            .collect::<Vec<ArcPoolTx>>();
        txs.sort_by_key(|a| core::cmp::Reverse(a.tip()));

        select_transactions(txs.into_iter(), block_gas_limit)
            .into_iter()
            .map(|tx| TxGas {
                limit: tx.script_gas_limit().unwrap_or_default(),
                tip: tx.tip(),
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
    #[case(999, vec![])]
    #[case(1000, vec![TxGas { tip: 5, limit: 1000 }])]
    #[case(2500, vec![TxGas { tip: 5, limit: 1000 }, TxGas { tip: 2, limit: 1000 }])]
    #[case(4000, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 }
    ])]
    #[case(5000, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 2, limit: 1000 }])
    ]
    #[case(6_000, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 }
    ])]
    #[case(7_000, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 },
        TxGas { tip: 2, limit: 1000 }
    ])]
    #[case(8_000, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 },
        TxGas { tip: 2, limit: 1000 },
        TxGas { tip: 1, limit: 1000 }
    ])]
    fn selector_prefers_highest_gas_txs_and_sorts(
        #[case] selection_limit: u64,
        #[case] expected_txs: Vec<TxGas>,
    ) {
        #[rustfmt::skip]
        let original = [
            TxGas { tip: 3, limit: 2000 },
            TxGas { tip: 1, limit: 1000 },
            TxGas { tip: 4, limit: 3000 },
            TxGas { tip: 5, limit: 1000 },
            TxGas { tip: 2, limit: 1000 },
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
            TxGas { tip: 3, limit: 2000 },
            TxGas { tip: 1, limit: 1000 },
            TxGas { tip: 4, limit: 3000 },
            TxGas { tip: 5, limit: 1000 },
            TxGas { tip: 2, limit: 1000 },
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
