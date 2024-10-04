//! This module is basically a fuzzer that throws random dependent transactions into the pool.
//! It checks that TxPool can't panic and that its graph and state are
//! correct(not in the unexpected state).
//! It relies on the `debug_assert` which are present in the code.

#![allow(non_snake_case)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::arithmetic_side_effects)]

use std::sync::Arc;

use crate::{
    config::{
        Config,
        PoolLimits,
    },
    selection_algorithms::Constraints,
    tests::universe::TestPoolUniverse,
};
use fuel_core_types::{
    fuel_tx::{
        field::{
            MaxFeeLimit,
            Tip,
        },
        ConsensusParameters,
        Finalizable,
        GasCosts,
        Input,
        Output,
        Script,
        TransactionBuilder,
        TxId,
        UtxoId,
    },
    fuel_types::AssetId,
    fuel_vm::checked_transaction::{
        Checked,
        IntoChecked,
    },
    services::txpool::{
        Metadata,
        PoolTransaction,
    },
};
use rand::{
    prelude::SliceRandom,
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[derive(Debug, Clone, Copy)]
struct Limits {
    max_inputs: usize,
    min_outputs: u16,
    max_outputs: u16,
    utxo_id_range: u8,
    gas_limit_range: u64,
    max_block_gas: u64,
}

fn transaction_with_random_inputs_and_outputs(
    limits: Limits,
    tip: u64,
    rng: &mut StdRng,
) -> (Checked<Script>, Metadata) {
    const AMOUNT: u64 = 10000;

    let mut consensus_parameters = ConsensusParameters::standard();
    consensus_parameters.set_gas_costs(GasCosts::free());

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.with_params(consensus_parameters.clone());

    let mut owner = [0u8; 32];
    owner[0] = 123;
    owner[1] = 222;
    let owner = owner.into();

    let mut random_ids;

    loop {
        let mut random_ids_unsorted = (0..limits.utxo_id_range)
            .map(|_| rng.gen_range(0..limits.utxo_id_range))
            .collect::<Vec<_>>();
        random_ids_unsorted.sort();
        random_ids_unsorted.dedup();

        if random_ids_unsorted.len() > 1 {
            random_ids_unsorted.shuffle(rng);
            random_ids = random_ids_unsorted;
            break;
        }
    }

    let tx_id_byte = random_ids.pop().expect("No random ids");
    let tx_id: TxId = [tx_id_byte; 32].into();
    let max_gas = rng.gen_range(0..limits.gas_limit_range);

    let inputs_count = limits.max_inputs.min(random_ids.len());
    let inputs_count = if inputs_count == 1 {
        1
    } else {
        rng.gen_range(1..inputs_count)
    };

    let inputs = random_ids.into_iter().take(inputs_count);

    for input_utxo_id in inputs {
        let output_index = rng.gen_range(0..limits.max_outputs);
        let utxo_id = UtxoId::new([input_utxo_id; 32].into(), output_index);

        builder.add_input(Input::coin_signed(
            utxo_id,
            owner,
            AMOUNT * 2,
            AssetId::BASE,
            Default::default(),
            Default::default(),
        ));
    }

    // We can't have more outputs than inputs.
    let outputs = rng
        .gen_range(limits.min_outputs..limits.max_outputs)
        .min(inputs_count as u16);

    for _ in 0..outputs {
        let output = Output::coin(owner, AMOUNT, AssetId::BASE);
        builder.add_output(output);
    }

    builder.add_witness(Default::default());

    let mut tx = builder.finalize();
    tx.set_tip(tip);
    tx.set_max_fee_limit(tip);

    let checked = tx
        .into_checked_basic(0u32.into(), &consensus_parameters)
        .unwrap();
    let metadata = Metadata::new_test(0, Some(max_gas), Some(tx_id));

    (checked, metadata)
}

fn stability_test(limits: Limits, config: Config) {
    use rand::RngCore;
    let seed = rand::thread_rng().next_u64();

    let result = std::panic::catch_unwind(|| {
        stability_test_with_seed(seed, limits, config);
    });

    if let Err(err) = result {
        tracing::error!("Stability test failed with seed: {}; err: {:?}", seed, err);
        panic!("Stability test failed with seed: {}; err: {:?}", seed, err);
    }
}

fn stability_test_with_seed(seed: u64, limits: Limits, config: Config) {
    let mut rng = StdRng::seed_from_u64(seed);

    let mut errors = 0;

    let mut universe = TestPoolUniverse::default().config(config);
    universe.build_pool();
    let txpool = universe.get_pool();

    for tip in 0..ROUNDS_PER_TXPOOL {
        let (checked, metadata) =
            transaction_with_random_inputs_and_outputs(limits, tip as u64, &mut rng);
        let pool_tx = PoolTransaction::Script(checked, metadata);

        let result = txpool.write().insert(Arc::new(pool_tx));
        errors += result.is_err() as usize;

        if tip % 10 == 0 {
            txpool.read().storage.check_integrity();
        }
    }

    assert_ne!(ROUNDS_PER_TXPOOL, errors);

    loop {
        let result = txpool
            .write()
            .extract_transactions_for_block(Constraints {
                max_gas: limits.max_block_gas,
                maximum_txs: u16::MAX,
                maximum_block_size: u32::MAX,
            })
            .unwrap();

        if result.is_empty() {
            break
        }

        txpool.read().storage.check_integrity();
    }

    {
        let txpool = txpool.read();
        assert_eq!(txpool.current_gas, 0);
        assert_eq!(txpool.current_bytes_size, 0);
        assert!(txpool.tx_id_to_storage_id.is_empty());
        assert!(txpool.selection_algorithm.is_empty());
        assert!(txpool.storage.is_empty());
        assert!(txpool.collision_manager.is_empty());
    }
}

const ROUNDS_PER_TEST: usize = 30;
const ROUNDS_PER_TXPOOL: usize = 1500;

#[test]
fn stability_test__average_transactions() {
    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 4,
        min_outputs: 1,
        max_outputs: 4,
        utxo_id_range: 12,
        gas_limit_range: 1000,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__many_non_conflicting_dependencies() {
    let config = Config {
        utxo_validation: false,
        max_txs_chain_count: 32,
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 3,
        min_outputs: 2,
        max_outputs: 3,
        utxo_id_range: 128,
        gas_limit_range: 10_000,
        max_block_gas: 100_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__many_dependencies() {
    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 200,
        min_outputs: 1,
        max_outputs: 10,
        utxo_id_range: 255,
        gas_limit_range: 1_000,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__many_conflicting_transactions_with_different_priority() {
    let config = Config {
        utxo_validation: false,
        pool_limits: PoolLimits {
            max_txs: 32,
            max_gas: 80_000,
            max_bytes_size: 1_000_000,
        },
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 200,
        min_outputs: 1,
        max_outputs: 10,
        utxo_id_range: 255,
        gas_limit_range: 10_000,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__long_chain_of_transactions() {
    let config = Config {
        utxo_validation: false,
        max_txs_chain_count: 128,
        pool_limits: PoolLimits {
            max_txs: 1_000,
            max_gas: 80_000,
            max_bytes_size: 1_000_000_000,
        },
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 2,
        min_outputs: 1,
        max_outputs: 2,
        utxo_id_range: 255,
        gas_limit_range: 100,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__long_chain_of_transactions_with_conflicts() {
    let config = Config {
        utxo_validation: false,
        max_txs_chain_count: 32,
        pool_limits: PoolLimits {
            max_txs: 1_000,
            max_gas: 80_000,
            max_bytes_size: 1_000_000_000,
        },
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 2,
        min_outputs: 1,
        max_outputs: 2,
        utxo_id_range: 255,
        gas_limit_range: 10_000,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}

#[test]
fn stability_test__wide_chain_of_transactions_with_conflicts() {
    let config = Config {
        utxo_validation: false,
        max_txs_chain_count: 32,
        pool_limits: PoolLimits {
            max_txs: 1_000,
            max_gas: 80_000,
            max_bytes_size: 1_000_000_000,
        },
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 5,
        min_outputs: 1,
        max_outputs: 5,
        utxo_id_range: 255,
        gas_limit_range: 10_000,
        max_block_gas: 10_000,
    };

    for _ in 0..ROUNDS_PER_TEST {
        stability_test(limit, config.clone());
    }
}
