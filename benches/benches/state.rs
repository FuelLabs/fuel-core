use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core::database::storage::{
    ContractsAssetsMerkleMetadata,
    SparseMerkleMetadata,
};
use fuel_core_benches::{
    VmBench,
    VmBenchPrepared,
};
use fuel_core_storage::{
    transactional::Transaction,
    ContractsAssetKey,
    StorageAsMut,
    StorageAsRef,
    StorageMutate,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        Instruction,
    },
    fuel_merkle::sparse::MerkleTree,
    fuel_tx::Bytes32,
    fuel_types::{
        AssetId,
        ContractId,
    },
};
use std::iter;

impl From<ContractsAssetKey> for [u8; 32] {
    fn from(value: ContractsAssetKey) -> Self {
        value.asset_id
    }
}

fn state(c: &mut Criterion) {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    let mut group = c.benchmark_group("state");

    let rng = &mut StdRng::seed_from_u64(0xF00DF00D);
    let contract: ContractId = rng.gen();

    let op = op::bal(0x10, 0x10, 0x11);

    let input = VmBench::new(op).with_prepare_db(move |mut db| {
        let gen = || {
            // Generate new assets for the existing contract
            let asset_id: AssetId = rng.gen();
            asset_id
        };

        let n = 1_000;
        let asset_keys = iter::repeat_with(gen).take(n);
        let asset_values = iter::repeat(100_u32.to_be_bytes()).take(n);
        let mut asset_key_values = asset_keys.zip(asset_values).into_iter();

        // Insert the key values into the database while building the Merkle tree
        let tree = MerkleTree::from_set(&mut db, asset_key_values)?;

        // Create the relevant Merkle metadata generated for the key values
        let root = tree.root();
        let metadata = SparseMerkleMetadata { root };
        db.storage_as_mut::<ContractsAssetsMerkleMetadata>()
            .insert(&contract, &metadata)?;

        Ok(db)
    });

    let mut i = input.prepare().expect("failed to prepare bench");

    group.bench_function("state", move |b| {
        b.iter_custom(|iters| {
            let VmBenchPrepared {
                vm,
                instruction,
                diff,
            } = &mut i;
            let original_db = vm.as_mut().database_mut().clone();
            let mut db_txn = {
                let db = vm.as_mut().database_mut();
                let db_txn = db.transaction();
                *db = db_txn.as_ref().clone();
                db_txn
            };

            let start = std::time::Instant::now();
            for _ in 0..iters {
                match instruction {
                    Instruction::CALL(call) => {
                        let (ra, rb, rc, rd) = call.unpack();
                        vm.prepare_call(ra, rb, rc, rd).unwrap();
                    }
                    _ => {
                        black_box(vm.instruction(*instruction).unwrap());
                    }
                }
                vm.reset_vm_state(diff);
            }
            db_txn.commit().unwrap();
            *vm.as_mut().database_mut() = original_db;
            start.elapsed()
        })
    });

    group.finish();
}

criterion_group!(benches, state);
criterion_main!(benches);
