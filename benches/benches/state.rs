use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core::database::Database;
use fuel_core_storage::vm_storage::VmStorage;
use fuel_core_types::{
    blockchain::header::GeneratedConsensusFields,
    fuel_tx::Bytes32,
    fuel_types::ContractId,
    fuel_vm::InterpreterStorage,
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

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn setup(db: &mut Database, contract: &ContractId, n: usize) {
    let mut rng_keys = thread_rng();
    let gen_keys = || -> Bytes32 { rng_keys.gen() };
    let state_keys = iter::repeat_with(gen_keys).take(n);

    let mut rng_values = thread_rng();
    let gen_values = || -> Bytes32 { rng_values.gen() };
    let state_values = iter::repeat_with(gen_values).take(n);

    // State key-values
    let state_key_values = state_keys.zip(state_values);

    db.init_contract_state(contract, state_key_values)
        .expect("Failed to initialize contract state");
}

fn insert_state_single_contract_database(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmStorage::default();
            let contract: ContractId = rng.gen();
            setup(db.database_mut(), &contract, n);
            let outer = db.database_mut().transaction();
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmStorage::new::<GeneratedConsensusFields>(
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

    let mut group = c.benchmark_group("insert state single contract database");

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

fn insert_state_single_contract_transaction(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmStorage::<Database>::default();
            let contract: ContractId = rng.gen();
            let mut outer = db.database_mut().transaction();
            setup(outer.as_mut(), &contract, n);
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmStorage::new::<GeneratedConsensusFields>(
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

    let mut group = c.benchmark_group("insert state single contract transaction");

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

fn insert_state_multiple_contracts_database(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmStorage::<Database>::default();
            for _ in 0..n {
                let contract: ContractId = rng.gen();
                setup(db.database_mut(), &contract, 1);
            }
            let outer = db.database_mut().transaction();
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let contract: ContractId = rng.gen();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmStorage::new::<GeneratedConsensusFields>(
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

    let mut group = c.benchmark_group("insert state multiple contracts database");

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

fn insert_state_multiple_contracts_transaction(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: usize| {
        group.bench_function(name, |b| {
            let mut db = VmStorage::<Database>::default();
            let mut outer = db.database_mut().transaction();
            for _ in 0..n {
                let contract: ContractId = rng.gen();
                setup(outer.as_mut(), &contract, 1);
            }
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let contract: ContractId = rng.gen();
                for _ in 0..iters {
                    let mut inner = outer.transaction();
                    let mut inner_db = VmStorage::new::<GeneratedConsensusFields>(
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

    let mut group = c.benchmark_group("insert state multiple contracts transaction");

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

criterion_group!(
    benches,
    insert_state_single_contract_database,
    insert_state_single_contract_transaction,
    insert_state_multiple_contracts_database,
    insert_state_multiple_contracts_transaction
);
criterion_main!(benches);
