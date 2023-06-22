use criterion::{
    black_box,
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
    ContractsStateKey,
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
    Rng,
    SeedableRng,
};
use std::iter;

fn setup(rng: &mut StdRng, db: &mut VmDatabase, contract: &ContractId, n: usize) {
    let gen = || -> Bytes32 rng.gen();
    let state_keys = iter::repeat_with(gen).take(n);
    let state_values = iter::repeat_with(gen).take(n);
    let state_key_values = state_keys.zip(state_values).into_iter();

    // Insert the key-values into the database while building the Merkle tree
    let tree: MerkleTree<ContractsStateMerkleData, _> =
        MerkleTree::from_set(db, state_key_values).expect("Failed to setup DB");

    // Create the relevant Merkle metadata generated for the key values
    let root = tree.root();
    let metadata = SparseMerkleMetadata { root };

    let db = tree.into_storage();
    db.storage_as_mut::<ContractsStateMerkleMetadata>()
        .insert(&contract, &metadata)
        .expect("Failed to create Merkle metadata");
}

fn state(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);

    let state: Bytes32 = rng.gen();
    let value: Bytes32 = rng.gen();

    let mut group = c.benchmark_group("state");

    group.bench_function("insert state with 0 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut rng, &mut db, &contract, 0);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert asset");
            }

            start.elapsed()
        })
    });

    group.bench_function("insert state with 1,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut rng, &mut db, &contract, 1_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert asset");
            }

            start.elapsed()
        })
    });

    group.bench_function("insert state with 100,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut rng, &mut db, &contract, 100_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert asset");
            }

            start.elapsed()
        })
    });

    group.bench_function("insert state with 10,000,000 preexisting entries", |b| {
        let mut db = VmDatabase::default();
        let contract: ContractId = rng.gen();
        setup(&mut rng, &mut db, &contract, 10_000_000);
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                db.merkle_contract_state_insert(&contract, &state, &value)
                    .expect("failed to insert asset");
            }

            start.elapsed()
        })
    });

    group.finish();
}

criterion_group!(benches, state);
criterion_main!(benches);
