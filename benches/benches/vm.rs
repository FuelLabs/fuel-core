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

use crate::vm_initialization::vm_initialization;
use contract::*;
use fuel_core_benches::*;
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

            let mut total = core::time::Duration::ZERO;
            for _ in 0..iters {
                let original_db = vm.as_mut().database_mut().clone();
                // Simulates the block production/validation with three levels of database transaction.
                let block_database_tx = original_db.transaction().as_ref().clone();
                let tx_database_tx = block_database_tx.transaction().as_ref().clone();
                let vm_tx_database_tx = tx_database_tx.transaction().as_ref().clone();
                *vm.as_mut().database_mut() = vm_tx_database_tx;

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
                // restore original db
                *vm.as_mut().database_mut() = original_db;
            }
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
