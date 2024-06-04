mod contract;
mod utils;
mod vm_initialization;
mod vm_set;

use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use std::sync::Arc;

use crate::vm_initialization::vm_initialization;
use contract::*;
use fuel_core::database::GenesisDatabase;
use fuel_core_benches::*;
use fuel_core_storage::transactional::IntoTransaction;
use fuel_core_types::fuel_asm::Instruction;
use vm_set::*;

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub fn run_group_ref<I>(group: &mut BenchmarkGroup<WallTime>, id: I, bench: VmBench)
where
    I: AsRef<str>,
{
    let mut i = bench.prepare().expect("failed to prepare bench");
    group.bench_function::<_, _>(id.as_ref(), move |b| {
        b.iter_custom(|iters| {
            let VmBenchPrepared {
                vm,
                instruction,
                diff,
            } = &mut i;

            let clock = quanta::Clock::new();

            let original_db = vm.as_mut().database_mut().clone();
            let original_memory = vm.memory().clone();
            // During block production/validation for each state, which may affect the state of the database,
            // we create a new storage transaction. The code here simulates the same behavior to have
            // the same nesting level and the same performance.
            let block_database_tx = original_db.clone().into_transaction();
            let relayer_database_tx = block_database_tx.into_transaction();
            let tx_database_tx = relayer_database_tx.into_transaction();
            let database = GenesisDatabase::new(Arc::new(tx_database_tx));
            *vm.as_mut().database_mut() = database.into_transaction();

            let mut total = core::time::Duration::ZERO;
            for _ in 0..iters {
                vm.memory_mut().clone_from(&original_memory);
                let start = black_box(clock.raw());
                match instruction {
                    Instruction::CALL(call) => {
                        let (ra, rb, rc, rd) = call.unpack();
                        black_box(vm.prepare_call(ra, rb, rc, rd)).unwrap();
                    }
                    _ => {
                        black_box(vm.instruction(*instruction).unwrap());
                    }
                }
                black_box(&vm);
                let end = black_box(clock.raw());
                total += clock.delta(start, end);
                vm.reset_vm_state(diff);
                // Reset database changes.
                vm.as_mut().database_mut().reset_changes();
            }
            *vm.as_mut().database_mut() = original_db;
            *vm.memory_mut() = original_memory;
            total
        })
    });
}

fn vm(c: &mut Criterion) {
    alu::run(c);
    crypto::run(c);
    flow::run(c);
    mem::run(c);
    blockchain::run(c);
    contract_root(c);
    state_root(c);
    vm_initialization(c);
}

criterion_group!(benches, vm);
criterion_main!(benches);

// If you want to debug the benchmarks, you can run them with code below:
// But first you need to comment `criterion_group` and `criterion_main` macros above.
//
// fn main() {
//     let criterio = Criterion::default();
//     let mut criterio = criterio.with_filter("vm_initialization");
//     alu::run(&mut criterio);
//     crypto::run(&mut criterio);
//     flow::run(&mut criterio);
//     mem::run(&mut criterio);
//     blockchain::run(&mut criterio);
//     contract_root(&mut criterio);
//     state_root(&mut criterio);
//     vm_initialization(&mut criterio);
// }
//
// #[test]
// fn dummy() {}
