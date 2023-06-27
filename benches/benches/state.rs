use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core::database::vm_database::VmDatabase;
use fuel_core_storage::InterpreterStorage;
use fuel_core_types::{
    blockchain::header::GeneratedConsensusFields,
    fuel_tx::Bytes32,
    fuel_types::ContractId,
};
use rand::{
    rngs::StdRng,
    thread_rng,
    Rng,
    SeedableRng,
};
use std::{
    iter,
    time::Duration,
};

fn setup(db: &mut VmDatabase, contract: &ContractId, n: usize) {
    let mut rng_keys = thread_rng();
    let gen_keys = || -> Bytes32 { rng_keys.gen() };
    let state_keys = iter::repeat_with(gen_keys).take(n);

    let mut rng_values = thread_rng();
    let gen_values = || -> Bytes32 { rng_values.gen() };
    let state_values = iter::repeat_with(gen_values).take(n);

    // State key-values
    let state_key_values = state_keys.zip(state_values);

    db.database_mut()
        .init_contract_state(contract, state_key_values)
        .expect("Failed to initialize contract state");
}

fn state_single_contract(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmDatabase::default();
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, n);
            let outer = db.database_mut().transaction();
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmDatabase::new::<GeneratedConsensusFields>(
                        inner.as_mut().clone(),
                        &Default::default(),
                        Default::default(),
                    );
                    let start = std::time::Instant::now();
                    inner_db
                        .merkle_contract_state_insert(&contract, &state, &value)
                        .expect("failed to insert state into transaction");
                    elapsed_time += start.elapsed();
                }
                elapsed_time
            });
        });
    };

    let mut group = c.benchmark_group("state single contract");

    bench_state(&mut group, "insert state with 0 preexisting entries", 0);
    bench_state(&mut group, "insert state with 1 preexisting entry", 1);
    bench_state(&mut group, "insert state with 10 preexisting entries", 10);
    bench_state(&mut group, "insert state with 100 preexisting entries", 100);
    bench_state(
        &mut group,
        "insert state with 1,000 preexisting entries",
        1_000,
    );
    bench_state(
        &mut group,
        "insert state with 10,000 preexisting entries",
        10_000,
    );
    bench_state(
        &mut group,
        "insert state with 100,000 preexisting entries",
        100_000,
    );
    bench_state(
        &mut group,
        "insert state with 1,000,000 preexisting entries",
        1_000_000,
    );

    group.finish();
}

fn state_multiple_contracts(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmDatabase::default();
            for _ in 0..n {
                let contract: ContractId = rng.gen();
                setup(&mut db, &contract, 1);
            }
            let outer = db.database_mut().transaction();
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let contract: ContractId = rng.gen();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmDatabase::new::<GeneratedConsensusFields>(
                        inner.as_mut().clone(),
                        &Default::default(),
                        Default::default(),
                    );
                    let start = std::time::Instant::now();
                    inner_db
                        .merkle_contract_state_insert(&contract, &state, &value)
                        .expect("failed to insert state into transaction");
                    elapsed_time += start.elapsed();
                }
                elapsed_time
            })
        });
    };

    let mut group = c.benchmark_group("state multiple contracts");

    bench_state(&mut group, "insert state with 0 preexisting entries", 0);
    bench_state(&mut group, "insert state with 1 preexisting entry", 1);
    bench_state(&mut group, "insert state with 10 preexisting entries", 10);
    bench_state(&mut group, "insert state with 100 preexisting entries", 100);
    bench_state(
        &mut group,
        "insert state with 1,000 preexisting entries",
        1_000,
    );
    bench_state(
        &mut group,
        "insert state with 10,000 preexisting entries",
        10_000,
    );
    bench_state(
        &mut group,
        "insert state with 100,000 preexisting entries",
        100_000,
    );
    bench_state(
        &mut group,
        "insert state with 1,000,000 preexisting entries",
        1_000_000,
    );

    group.finish();
}

criterion_group!(benches, state_single_contract, state_multiple_contracts);
criterion_main!(benches);
