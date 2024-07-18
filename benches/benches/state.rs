use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core::{
    database::{
        database_description::on_chain::OnChain,
        state::StateInitializer,
        Database,
        GenesisStage,
    },
    state::{
        data_source::DataSource,
        generic_database::GenericDatabase,
        historical_rocksdb::{
            description::Historical,
            HistoricalRocksDB,
            StateRewindPolicy,
        },
        rocks_db::RocksDb,
    },
};
use fuel_core_storage::{
    transactional::{
        IntoTransaction,
        ReadTransaction,
        WriteTransaction,
    },
    vm_storage::VmStorage,
};
use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            ConsensusHeader,
        },
        primitives::Empty,
    },
    fuel_tx::Bytes32,
    fuel_types::ContractId,
    fuel_vm::InterpreterStorage,
};
use quanta::Instant;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    iter,
    sync::Arc,
    thread::sleep,
    time::Duration,
};

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn i_to_bytes32(i: u32) -> Bytes32 {
    let mut bytes = [0u8; 32];
    bytes[0..4].copy_from_slice(&i.to_be_bytes());
    Bytes32::from(bytes)
}

fn setup<D>(db: &mut D, contract: &ContractId, rng: &mut StdRng, n: u32)
where
    D: StateInitializer,
{
    // let gen_bytes32 = || -> Bytes32 { rng.gen() };
    // let state_keys = (0..n).into_iter().map(i_to_bytes32).collect::<Vec<_>>();
    let mut gen_bytes32 = || -> Bytes32 { rng.gen() };
    let state_keys = iter::repeat_with(&mut gen_bytes32)
        .take(n as usize)
        .collect::<Vec<_>>();

    let state_values = iter::repeat_with(gen_bytes32)
        .map(|b| b.to_vec())
        .take(n as usize);

    // State key-values
    let state_key_values = state_keys.into_iter().zip(state_values).collect::<Vec<_>>();

    db.init_contract_state(contract, state_key_values.into_iter())
        .expect("Failed to initialize contract state");
}

fn insert_state_single_contract_database(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: u32| {
        let _cache = Some(16usize * 1024 * 1024 * 1024);
        let db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let path = db.path().clone();
        let historical_db =
            HistoricalRocksDB::new(db, StateRewindPolicy::NoRewind).unwrap();
        let data = Arc::new(historical_db);
        let mut db = GenericDatabase::from_storage(
            DataSource::<OnChain, GenesisStage>::new(data, GenesisStage::default()),
        );

        let start = Instant::now();
        let contract: ContractId = rng.gen();
        let mut tx = db.write_typed_transaction();
        setup(&mut tx, &contract, &mut rng, n);
        tx.commit().expect("failed to commit transaction");
        println!("setup time: {:?}", start.elapsed());
        println!("setup time: {:?}", fs_extra::dir::get_size(path));

        let size = 20;
        let mut gen_bytes32 = || -> Bytes32 { rng.gen() };
        // let state_keys = (n..n + size)
        //     .into_iter()
        //     .map(i_to_bytes32)
        //     .collect::<Vec<_>>();
        let state_keys = iter::repeat_with(&mut gen_bytes32)
            .take(n as usize)
            .collect::<Vec<_>>();

        let state_values = iter::repeat_with(&mut gen_bytes32)
            .map(|b| b.to_vec())
            .take(size as usize);

        // State key-values
        let state_key_values: Vec<_> = state_keys.into_iter().zip(state_values).collect();

        group.sample_size(50).bench_function(name, |b| {
            let outer = db.write_transaction();
            b.iter(|| {
                let inner = outer.read_typed_transaction();
                let mut inner_db = VmStorage::new(
                    inner,
                    &ConsensusHeader::<Empty>::default(),
                    &ApplicationHeader::<Empty>::default(),
                    Default::default(),
                );
                // for _ in 0..size {
                //     let state: Bytes32 = rng.gen();
                //     let value: Bytes32 = rng.gen();
                //     inner_db
                //         .contract_state_insert(&contract, &state, value.as_slice())
                //         .expect("failed to insert state into transaction");
                // }
                for (state, value) in &state_key_values {
                    inner_db
                        .contract_state_insert(&contract, state, value.as_slice())
                        .expect("failed to insert state into transaction");
                }
            });
        });
    };

    let mut group = c.benchmark_group("insert state single contract database");

    // bench_state(&mut group, "insert state with 0 preexisting entries", 0);
    // bench_state(&mut group, "insert state with 1 preexisting entry", 1);
    // bench_state(&mut group, "insert state with 10 preexisting entries", 10);
    // bench_state(&mut group, "insert state with 100 preexisting entries", 100);
    // bench_state(
    //     &mut group,
    //     "insert state with 1,000 preexisting entries",
    //     1_000,
    // );
    // bench_state(
    //     &mut group,
    //     "insert state with 10,000 preexisting entries",
    //     10_000,
    // );
    bench_state(
        &mut group,
        "insert state with 100,000 preexisting entries",
        1_000_000,
    );
    // bench_state(
    //     &mut group,
    //     "insert state with 1,000,000 preexisting entries",
    //     1_000_000,
    // );

    group.finish();
}

fn insert_state_single_contract_transaction(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: u32| {
        group.bench_function(name, |b| {
            let db = Database::<OnChain>::default();
            let contract: ContractId = rng.gen();
            let mut outer = db.into_transaction();
            setup(&mut outer, &contract, &mut rng, n);
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let inner = outer.read_transaction();
                    let mut inner_db = VmStorage::new(
                        inner,
                        &ConsensusHeader::<Empty>::default(),
                        &ApplicationHeader::<Empty>::default(),
                        Default::default(),
                    );
                    let start = std::time::Instant::now();
                    inner_db
                        .contract_state_insert(&contract, &state, value.as_slice())
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

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: u32| {
        group.bench_function(name, |b| {
            let mut db = Database::<OnChain>::default();
            for _ in 0..n {
                let contract: ContractId = rng.gen();
                setup(&mut db, &contract, &mut rng, 1);
            }
            let outer = db.into_transaction();
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let contract: ContractId = rng.gen();
                for _ in 0..iters {
                    let inner = outer.read_transaction();
                    let mut inner_db = VmStorage::new(
                        inner,
                        &ConsensusHeader::<Empty>::default(),
                        &ApplicationHeader::<Empty>::default(),
                        Default::default(),
                    );
                    let start = std::time::Instant::now();
                    inner_db
                        .contract_state_insert(&contract, &state, value.as_slice())
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

    let mut bench_state = |group: &mut BenchmarkGroup<WallTime>, name: &str, n: u32| {
        group.bench_function(name, |b| {
            let db = Database::<OnChain>::default();
            let mut outer = db.into_transaction();
            for _ in 0..n {
                let contract: ContractId = rng.gen();
                setup(&mut outer, &contract, &mut rng, 1);
            }
            b.iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let contract: ContractId = rng.gen();
                for _ in 0..iters {
                    let inner = outer.read_transaction();
                    let mut inner_db = VmStorage::new(
                        inner,
                        &ConsensusHeader::<Empty>::default(),
                        &ApplicationHeader::<Empty>::default(),
                        Default::default(),
                    );
                    let start = std::time::Instant::now();
                    inner_db
                        .contract_state_insert(&contract, &state, value.as_slice())
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

criterion_group!(benches, insert_state_single_contract_database,);
criterion_main!(benches);
