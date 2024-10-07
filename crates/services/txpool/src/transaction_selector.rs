use fuel_core_types::{
    fuel_types::Word,
    services::txpool::ArcPoolTx,
};

// transaction selection could use a plugin based approach in the
// future for block producers to customize block building (e.g. alternative priorities besides gas fees)

// Expects sorted by tip, highest first
pub fn select_transactions(
    includable_txs: impl Iterator<Item = ArcPoolTx>,
    max_gas: u64,
    block_transaction_size_limit: u32,
) -> impl Iterator<Item = ArcPoolTx> {
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.

    let mut used_block_gas_space: Word = 0;
    let mut used_block_size_space: u32 = 0;

    // The type of the index for the transaction is `u16`, so we need to
    // limit it to `MAX` value minus 1(because of the `Mint` transaction).
    let takes_txs = u16::MAX - 1;

    // Pick as many transactions as we can fit into the block (greedy),
    // respecting both gas and size limit
    includable_txs
        .filter(move |tx| {
            let tx_block_gas_space = tx.max_gas();
            let tx_block_size_space =
                tx.metered_bytes_size().try_into().unwrap_or(u32::MAX);

            let new_used_block_gas_space =
                used_block_gas_space.saturating_add(tx_block_gas_space);
            let new_used_block_size_space =
                used_block_size_space.saturating_add(tx_block_size_space);

            if new_used_block_gas_space <= max_gas
                && new_used_block_size_space <= block_transaction_size_limit
            {
                used_block_gas_space = new_used_block_gas_space;
                used_block_size_space = new_used_block_size_space;
                true
            } else {
                false
            }
        })
        .take(takes_txs as usize)
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
            rngs::ThreadRng,
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
        services::txpool::{
            Metadata,
            PoolTransaction,
        },
    };
    use itertools::Itertools;
    use std::sync::Arc;

    use super::*;

    const UNLIMITED_SIZE: u32 = u32::MAX;
    const UNLIMITED_GAS: u64 = u64::MAX;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct TxGas {
        pub tip: u64,
        pub limit: u64,
    }

    /// A test helper that generates set of txs with given gas prices and limits and runs
    /// `select_transactions` against that, returning the list of selected gas price, limit pairs
    fn make_txs_and_select(
        txs: &[TxGas],
        block_gas_limit: Word,
        block_transaction_size_limit: u32,
    ) -> Vec<TxGas> {
        let mut rng = thread_rng();
        let txs = make_txs(txs, &mut rng);

        select_transactions(
            txs.into_iter(),
            block_gas_limit,
            block_transaction_size_limit,
        )
        .map(|tx| TxGas {
            limit: tx.script_gas_limit().unwrap_or_default(),
            tip: tx.tip(),
        })
        .collect()
    }

    /// A test helper that generates set of txs with given gas prices and limits.
    fn make_txs(txs: &[TxGas], rng: &mut ThreadRng) -> Vec<Arc<PoolTransaction>> {
        let fee_params = FeeParameters::default()
            .with_gas_per_byte(0)
            .with_gas_price_factor(1);

        let mut txs = txs
            .iter()
            .map(|tx_gas| {
                let mut script_builder = TransactionBuilder::script(
                    vec![op::ret(RegId::ONE)].into_iter().collect(),
                    vec![],
                );

                let script = script_builder
                    .tip(tx_gas.tip)
                    .script_gas_limit(tx_gas.limit)
                    .add_unsigned_coin_input(
                        SecretKey::random(rng),
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
                    .with_gas_costs(GasCosts::free());

                // Ensure there is some randomness in the transaction size
                let witnesses_count = rng.gen_range(0..=50);
                for _ in 0..=witnesses_count {
                    script.add_witness(
                        std::iter::repeat(0)
                            .take(rng.gen_range(0..=100))
                            .collect_vec()
                            .into(),
                    );
                }

                // The block producer assumes transactions are already checked
                // so it doesn't need to compute valid sigs for tests
                PoolTransaction::Script(
                    script.finalize_checked_basic(Default::default()),
                    Metadata::new(ConsensusParametersVersion::MIN, 0, 0),
                )
            })
            .map(Arc::new)
            .collect::<Vec<ArcPoolTx>>();
        txs.sort_by_key(|a| core::cmp::Reverse(a.tip()));
        txs
    }

    #[test]
    fn selector_works_with_empty_input() {
        let selected = make_txs_and_select(&[], 1_000_000, 1_000_000);
        assert!(selected.is_empty());
    }

    #[rstest::rstest]
    #[test]
    #[case(999, UNLIMITED_SIZE, vec![])]
    #[case(1000, UNLIMITED_SIZE, vec![TxGas { tip: 5, limit: 1000 }])]
    #[case(2500, UNLIMITED_SIZE, vec![TxGas { tip: 5, limit: 1000 }, TxGas { tip: 2, limit: 1000 }])]
    #[case(4000, UNLIMITED_SIZE, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 }
    ])]
    #[case(5000, UNLIMITED_SIZE, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 2, limit: 1000 }])
    ]
    #[case(6_000, UNLIMITED_SIZE, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 }
    ])]
    #[case(7_000, UNLIMITED_SIZE, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 },
        TxGas { tip: 2, limit: 1000 }
    ])]
    #[case(8_000, UNLIMITED_SIZE, vec![
        TxGas { tip: 5, limit: 1000 },
        TxGas { tip: 4, limit: 3000 },
        TxGas { tip: 3, limit: 2000 },
        TxGas { tip: 2, limit: 1000 },
        TxGas { tip: 1, limit: 1000 }
    ])]

    // TODO[RC]: Does it actually prefer highest gas?
    fn selector_prefers_highest_gas_txs(
        #[case] selection_gas_limit: u64,
        #[case] selection_size_limit: u32,
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

        let selected =
            make_txs_and_select(&original, selection_gas_limit, selection_size_limit);
        assert_eq!(
            expected_txs, selected,
            "Wrong txs selected for max_gas: {selection_gas_limit}"
        );
    }

    #[test]
    fn selector_does_not_exceed_max_gas_per_block() {
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
                    let selected =
                        make_txs_and_select(perm.as_slice(), gas_limit, UNLIMITED_SIZE);
                    let total_gas: Word = selected.iter().map(|g| g.limit).sum();
                    assert!(total_gas <= gas_limit);
                }
            }
        }
    }

    #[rstest::fixture]
    pub fn block_transaction_size_limit_fixture() -> (Vec<Arc<PoolTransaction>>, u32) {
        const TOTAL_TXN_COUNT: usize = 1000;

        let mut rng = thread_rng();

        // Create `TOTAL_TXN_COUNT` transactions with random tip.
        let txs = make_txs(
            std::iter::repeat_with(|| TxGas {
                tip: rng.gen(),
                limit: 1,
            })
            .take(TOTAL_TXN_COUNT)
            .collect::<Vec<_>>()
            .as_slice(),
            &mut rng,
        );
        let total_txs_size = txs
            .iter()
            .map(|tx| tx.metered_bytes_size().try_into().unwrap_or(u32::MAX))
            .sum();

        (txs, total_txs_size)
    }

    #[rstest::rstest]
    fn unlimited_size_request_returns_all_transactions(
        block_transaction_size_limit_fixture: (Vec<Arc<PoolTransaction>>, u32),
    ) {
        let (txs, total_size) = block_transaction_size_limit_fixture;

        let selected =
            select_transactions(txs.into_iter(), UNLIMITED_GAS, UNLIMITED_SIZE);
        let selected_size: u32 = selected
            .map(|tx| tx.metered_bytes_size().try_into().unwrap_or(u32::MAX))
            .sum();
        assert_eq!(selected_size, total_size);
    }

    #[rstest::rstest]
    fn selection_respects_specified_size(
        block_transaction_size_limit_fixture: (Vec<Arc<PoolTransaction>>, u32),
    ) {
        let (txs, total_size) = block_transaction_size_limit_fixture;

        let mut rng = thread_rng();
        for selection_size_limit in
            std::iter::repeat_with(|| rng.gen_range(1..=total_size)).take(100)
        {
            let selected = select_transactions(
                txs.clone().into_iter(),
                UNLIMITED_GAS,
                selection_size_limit,
            );
            let selected_size: u32 = selected
                .map(|tx| tx.metered_bytes_size().try_into().unwrap_or(u32::MAX))
                .sum();
            assert!(selected_size <= selection_size_limit);
        }
    }

    #[rstest::rstest]
    fn selection_can_exactly_fit_the_requested_size(
        block_transaction_size_limit_fixture: (Vec<Arc<PoolTransaction>>, u32),
    ) {
        let (txs, _) = block_transaction_size_limit_fixture;

        let smallest_txn_size = txs
            .iter()
            .map(|tx| tx.metered_bytes_size().try_into().unwrap_or(u32::MAX))
            .sorted()
            .next()
            .expect("txs should not be empty");
        let selected =
            select_transactions(txs.into_iter(), UNLIMITED_GAS, smallest_txn_size);
        assert_eq!(selected.collect::<Vec<_>>().len(), 1);
    }
}
