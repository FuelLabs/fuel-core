use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core::database::{
    storage::{
        ContractsStateMerkleData,
        ContractsStateMerkleMetadata,
        SparseMerkleMetadata,
    },
    vm_database::VmDatabase,
};
use fuel_core_storage::{
    InterpreterStorage,
    StorageAsMut,
};
use fuel_core_types::{
    fuel_merkle::sparse::MerkleTree,
    fuel_tx::Bytes32,
    fuel_types::ContractId,
};
use rand::{
    rngs::StdRng,
    thread_rng,
    Rng,
    SeedableRng,
};
use std::iter;

fn setup(db: &mut VmDatabase, contract: &ContractId, n: usize) {
    let mut rng_keys = thread_rng();
    let gen_keys = || -> Bytes32 { rng_keys.gen() };
    let state_keys = iter::repeat_with(gen_keys).take(n);

    let mut rng_values = thread_rng();
    let gen_values = || -> Bytes32 { rng_values.gen() };
    let state_values = iter::repeat_with(gen_values).take(n);

    // State key-values
    let state_key_values = state_keys.zip(state_values);

    // Insert the key-values into the database while building the Merkle tree
    let tree: MerkleTree<ContractsStateMerkleData, _> =
        MerkleTree::from_set(db, state_key_values).expect("Failed to setup DB");

    // Create the relevant Merkle metadata generated for the key values
    let root = tree.root();
    let metadata = SparseMerkleMetadata { root };

    let db = tree.into_storage();
    db.storage_as_mut::<ContractsStateMerkleMetadata>()
        .insert(contract, &metadata)
        .expect("Failed to create Merkle metadata");
}

fn state_single_contract(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut group = c.benchmark_group("state single contract");

    group.bench_function("insert state with 0 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 0);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 1);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 10 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 10);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 100 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 100);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 1_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 10,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 10_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 100,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 100_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1,000,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut db, &contract, 1_000_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.finish();
}

fn state_multiple_contracts(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut group = c.benchmark_group("state multiple contracts");

    group.bench_function("insert state with 0 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..1 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 10 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..10 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 100 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..100 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..1_000 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 10,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..10_000 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 100,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..100_000 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.bench_function("insert state with 1,000,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        for _ in 0..1_000_000 {
            let contract: ContractId = rng.gen();
            setup(&mut db, &contract, 1);
        }
        b.iter_custom(|iters| {
            let contract: ContractId = rng.gen();
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert state");
            }
            start.elapsed()
        })
    });

    group.finish();
}

criterion_group!(benches, state_single_contract, state_multiple_contracts);
criterion_main!(benches);
