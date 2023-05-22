use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
};
use fuel_core::{
    executor::Executor,
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

fn txn(c: &mut Criterion) {
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
        config,
    };
    let mut test_data = InputOutputData::default();
    let mut data = Data::default();

    let mut execute = c.benchmark_group("transaction");

    let select = std::env::var_os("SELECT_BENCH")
        .map(|s| s.to_string_lossy().parse::<usize>().unwrap());

    if select.is_none() || matches!(select, Some(0)) {
        <out_ty::Void as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
        let t = into_script_txn(test_data);
        insert_into_db(&mut executor.database, &t, &mut data);
        measure_transaction(&executor, t, 5, "coin signed to void", &mut execute);
    }

    if select.is_none() || matches!(select, Some(1)) {
        let mut test_data = InputOutputData::default();
        <out_ty::Coin as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
        let t = into_script_txn(test_data);
        insert_into_db(&mut executor.database, &t, &mut data);
        measure_transaction(&executor, t, 5, "coin signed to coin", &mut execute);
    }

    if select.is_none() || matches!(select, Some(2)) {
        let mut test_data = InputOutputData::default();
        <out_ty::Variable as ValidTx<in_ty::CoinSigned>>::fill(
            &mut data,
            &mut test_data,
            5,
        );
        let t = into_script_txn(test_data);
        insert_into_db(&mut executor.database, &t, &mut data);
        measure_transaction(&executor, t, 5, "coin signed to variable", &mut execute);
    }

    if select.is_none() || matches!(select, Some(3)) {
        let mut test_data = InputOutputData::default();
        <out_ty::Change as ValidTx<in_ty::CoinSigned>>::fill(
            &mut data,
            &mut test_data,
            5,
        );
        let t = into_script_txn(test_data);
        insert_into_db(&mut executor.database, &t, &mut data);
        measure_transaction(&executor, t, 5, "coin signed to change", &mut execute);
    }

    if select.is_none() || matches!(select, Some(4)) {
        let mut test_data = InputOutputData::default();
        <(out_ty::Coin, out_ty::Contract) as ValidTx<(
            in_ty::MessageData,
            in_ty::Contract,
        )>>::fill(&mut data, &mut test_data, 5);
        let t = into_script_txn(test_data);
        insert_into_db(&mut executor.database, &t, &mut data);
        measure_transaction(&executor, t, 5, "coin signed to void", &mut execute);
    }
}

fn measure_transaction(
    executor: &Executor<MaybeRelayerAdapter>,
    transaction: Transaction,
    inputs: u64,
    name: &str,
    execute: &mut BenchmarkGroup<WallTime>,
) {
    let header = make_header();
    let block = PartialFuelBlock::new(header, vec![transaction]);
    let block = ExecutionBlock::Production(block);

    let result = executor.execute_without_commit(block.clone()).unwrap();
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

    execute.throughput(criterion::Throughput::Elements(inputs));
    execute.bench_with_input(BenchmarkId::new(name, inputs), &inputs, |b, _| {
        b.iter(|| {
            let result = executor.execute_without_commit(block.clone()).unwrap();
            assert!(result.result().skipped_transactions.is_empty());
        })
    });
}

criterion_group!(benches, txn);
criterion_main!(benches);
