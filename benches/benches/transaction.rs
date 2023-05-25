use std::sync::Arc;

use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
};
use fuel_core::{
    executor::{
        ExecutionOptions,
        Executor,
    },
    service::{
        adapters::MaybeRelayerAdapter,
        Config,
    },
};
use fuel_core_benches::{
    data::{
        in_out::*,
        make_header,
        Data,
    },
    Database,
};
use fuel_core_types::{
    blockchain::block::PartialFuelBlock,
    fuel_tx::Transaction,
    fuel_vm::GasCosts,
    services::executor::ExecutionBlock,
};

// Benchmark function to measure transaction performance.
fn txn(c: &mut Criterion) {
    // Initialize database, relayer, config, and executor.
    let database = Database::default();
    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
        relayer_synced: None,
        da_deploy_height: 0u64.into(),
    };
    let mut config = Config::local_node();
    config.chain_conf.gas_costs = GasCosts::free();
    config.utxo_validation = true;
    let mut executor = Executor {
        database,
        relayer,
        config: Arc::new(config.block_executor),
    };

    // Create arbitrary and deterministic data to use for transactions.
    let mut data = Data::default();

    // Create a benchmark group for transactions.
    let mut execute = c.benchmark_group("transaction");

    // Check if a specific benchmark is selected, otherwise run all.
    let select = std::env::var_os("SELECT_BENCH")
        .map(|s| s.to_string_lossy().parse::<usize>().unwrap());

    // Benchmark the baseline transaction.
    // This is the cheapest (CPU) valid transaction.
    if select.is_none() || matches!(select, Some(0)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "baseline",
            1,
            |data, io_data, num_inputs| {
                <out_ty::Void as ValidTx<in_ty::CoinSigned>>::fill(
                    data, io_data, num_inputs,
                );
            },
        )
    }

    if select.is_none() || matches!(select, Some(1)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "coin signed to void",
            5,
            |data, io_data, num_inputs| {
                <out_ty::Void as ValidTx<in_ty::CoinSigned>>::fill(
                    data, io_data, num_inputs,
                );
            },
        )
    }

    if select.is_none() || matches!(select, Some(2)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "coin signed to coin",
            5,
            |data, io_data, num_inputs| {
                <out_ty::Coin as ValidTx<in_ty::CoinSigned>>::fill(
                    data, io_data, num_inputs,
                );
            },
        )
    }

    if select.is_none() || matches!(select, Some(3)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "coin signed to variable",
            5,
            |data, io_data, num_inputs| {
                <out_ty::Variable as ValidTx<in_ty::CoinSigned>>::fill(
                    data, io_data, num_inputs,
                );
            },
        )
    }

    if select.is_none() || matches!(select, Some(4)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "coin signed to change",
            5,
            |data, io_data, num_inputs| {
                <out_ty::Change as ValidTx<in_ty::CoinSigned>>::fill(
                    data, io_data, num_inputs,
                );
            },
        )
    }

    if select.is_none() || matches!(select, Some(5)) {
        setup_and_measure(
            &mut executor,
            &mut execute,
            &mut data,
            "message data and coin signed to coin and contract",
            5,
            |data, io_data, num_inputs| {
                <(out_ty::Coin, out_ty::Contract) as ValidTx<(
                    in_ty::MessageData,
                    in_ty::Contract,
                )>>::fill(data, io_data, num_inputs);
            },
        )
    }
}

// Helper function to setup and measure a single transaction type.
fn setup_and_measure(
    executor: &mut Executor<MaybeRelayerAdapter>,
    execute: &mut BenchmarkGroup<WallTime>,
    data: &mut Data,
    name: &str,
    num_inputs: usize,
    fill: impl FnOnce(&mut Data, &mut InputOutputData, usize),
) {
    // Fill in the input and output data.
    let mut io_data = InputOutputData::default();
    fill(data, &mut io_data, num_inputs);

    // Create a script transaction from the input and output data.
    // TODO: Also test create.
    let t = into_script_txn(io_data);

    // Insert any data into the database.
    insert_into_db(&mut executor.database, &t, data);

    // Measure the transaction.
    measure_transaction(executor, t, num_inputs as u64, name, execute);
}

// Helper function to measure a single transaction type.
fn measure_transaction(
    executor: &Executor<MaybeRelayerAdapter>,
    transaction: Transaction,
    inputs: u64,
    name: &str,
    execute: &mut BenchmarkGroup<WallTime>,
) {
    // Create a block with the provided transaction.
    let header = make_header();
    let block = PartialFuelBlock::new(header, vec![transaction]);
    let block = ExecutionBlock::Production(block);

    // Perform a trial execution and check for errors.
    let result = executor
        .execute_without_commit(
            block.clone(),
            ExecutionOptions {
                utxo_validation: true,
            },
        )
        .unwrap();

    // If there are errors, print them and return early.
    if !result.result().skipped_transactions.is_empty() {
        let status = result.result().tx_status.clone();
        let errors = result
            .result()
            .skipped_transactions
            .iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        eprintln!("Transaction failed: {errors:?}, {status:?}");

        return
    }

    // Set the throughput metric for the current benchmark.
    execute.throughput(criterion::Throughput::Elements(inputs));

    // Run the benchmark with the given name and input count.
    execute.bench_with_input(BenchmarkId::new(name, inputs), &inputs, |b, _| {
        b.iter(|| {
            // Execute the transaction without committing it.
            // Assert there is never any errors.
            let result = executor
                .execute_without_commit(
                    block.clone(),
                    ExecutionOptions {
                        utxo_validation: true,
                    },
                )
                .unwrap();
            assert!(result.result().skipped_transactions.is_empty());
        })
    });
}

criterion_group!(benches, txn);
criterion_main!(benches);
