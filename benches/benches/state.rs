use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core_benches::{
    VmBench,
    VmBenchPrepared,
};
use fuel_core_storage::{
    transactional::Transaction,
    ContractsAssetsStorage,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        Instruction,
        RegId,
    },
    fuel_types::{
        AssetId,
        ContractId,
    },
};

fn state(c: &mut Criterion) {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    let mut group = c.benchmark_group("state");

    let rng = &mut StdRng::seed_from_u64(0xF00DF00D);
    let asset: AssetId = rng.gen();
    let contract: ContractId = rng.gen();

    // let op = op::bal(0x10, 0x10, 0x11);
    // let input = VmBench::new(op)
    //     .with_data(asset.iter().chain(contract.iter()).copied().collect())
    //     .with_prepare_script(vec![
    //         op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
    //         op::addi(0x11, 0x10, asset.len().try_into().unwrap()),
    //     ])
    //     .with_dummy_contract(contract)
    //     .with_prepare_db(move |mut db| {
    //         // let mut asset_inc = AssetId::zeroed();
    //         //
    //         // asset_inc.as_mut()[..8].copy_from_slice(&1_u64.to_be_bytes());
    //         //
    //         // db.merkle_contract_asset_id_balance_insert(&contract, &asset_inc, 1)?;
    //         //
    //         // db.merkle_contract_asset_id_balance_insert(&contract, &asset, 100)?;
    //
    //         Ok(db)
    //     });

    // let mut i = input.prepare().expect("failed to prepare bench");

    // group.bench_function("state", move |b| {
    //     b.iter_custom(|iters| {
    //         let VmBenchPrepared {
    //             vm,
    //             instruction,
    //             diff,
    //         } = &mut i;
    //         let original_db = vm.as_mut().database_mut().clone();
    //         let mut db_txn = {
    //             let db = vm.as_mut().database_mut();
    //             let db_txn = db.transaction();
    //             *db = db_txn.as_ref().clone();
    //             db_txn
    //         };
    //
    //         let start = std::time::Instant::now();
    //         for _ in 0..iters {
    //             match instruction {
    //                 Instruction::CALL(call) => {
    //                     let (ra, rb, rc, rd) = call.unpack();
    //                     vm.prepare_call(ra, rb, rc, rd).unwrap();
    //                 }
    //                 _ => {
    //                     black_box(vm.instruction(*instruction).unwrap());
    //                 }
    //             }
    //             vm.reset_vm_state(diff);
    //         }
    //         db_txn.commit().unwrap();
    //         *vm.as_mut().database_mut() = original_db;
    //         start.elapsed()
    //     })
    // });

    group.finish();
}

criterion_group!(benches, state);
criterion_main!(benches);
