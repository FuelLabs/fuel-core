mod contract;
mod utils;
mod vm_initialization;
mod vm_set;

use criterion::{
    BenchmarkGroup,
    Criterion,
    black_box,
    criterion_group,
    criterion_main,
    measurement::WallTime,
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

/// Measures a single instruction using `iter_custom` with production-like
/// storage nesting.
///
/// `cold` controls the between-iteration reset strategy:
///
/// * `false` (**hot**) – after each timed iteration `reset_vm_state` is called
///   to restore the diff and `reset_changes` clears pending DB writes.
///   `reset_vm_state` either leaves read-cache entries intact (read-only ops)
///   or re-warms them with the pre-instruction values (write ops), so every
///   iteration after the first hits the in-memory `storage_slot_cache`.
///
/// * `true` (**cold**) – the VM is snapshotted *after* DB setup but *before*
///   any timed execution, then restored from that snapshot before each
///   iteration.  The snapshot carries an empty `storage_slot_cache`, so every
///   measured iteration pays the cold-read cost.
fn run_group_ref_impl<I>(
    group: &mut BenchmarkGroup<WallTime>,
    id: I,
    bench: VmBench,
    cold: bool,
) where
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
            // During block production/validation for each state, which may affect the state of the database,
            // we create a new storage transaction. The code here simulates the same behavior to have
            // the same nesting level and the same performance.
            let block_database_tx = original_db.clone().into_transaction();
            let relayer_database_tx = block_database_tx.into_transaction();
            let tx_database_tx = relayer_database_tx.into_transaction();
            let database = GenesisDatabase::new(Arc::new(tx_database_tx));
            *vm.as_mut().database_mut() = database.into_transaction();

            // For cold mode: snapshot the VM (with empty storage_slot_cache)
            // before any timed execution so each iteration can be reset to it.
            let cold_vm = cold.then(|| vm.clone());

            let mut total = core::time::Duration::ZERO;
            for _ in 0..iters {
                if let Some(cold_vm) = &cold_vm {
                    *vm = cold_vm.clone();
                }

                let start = black_box(clock.raw());
                match instruction {
                    Instruction::CALL(call) => {
                        let (ra, rb, rc, rd) = call.unpack();
                        black_box(vm.prepare_call(ra, rb, rc, rd)).unwrap();
                    }
                    _ => {
                        black_box(vm.instruction::<_, false>(*instruction).unwrap());
                    }
                }
                black_box(&vm);
                let end = black_box(clock.raw());
                total += clock.delta(start, end);

                if cold_vm.is_none() {
                    vm.reset_vm_state(diff);
                    // Reset database changes.
                    vm.as_mut().database_mut().reset_changes();
                }
            }
            *vm.as_mut().database_mut() = original_db;
            total
        })
    });
}

/// Benchmarks `bench` with a warm `storage_slot_cache` (hot reads).
///
/// After criterion's warm-up phase every sample sees pre-populated cache
/// entries, measuring steady-state hot-path performance.
pub fn run_group_ref<I>(group: &mut BenchmarkGroup<WallTime>, id: I, bench: VmBench)
where
    I: AsRef<str>,
{
    run_group_ref_impl(group, id, bench, false);
}

/// Benchmarks `bench` with a cold `storage_slot_cache` (cold reads).
///
/// The VM is restored from a pre-execution snapshot before each timed
/// iteration so every sample pays the first-access storage cost, regardless
/// of criterion's warm-up iterations.
pub fn run_group_ref_cold<I>(group: &mut BenchmarkGroup<WallTime>, id: I, bench: VmBench)
where
    I: AsRef<str>,
{
    run_group_ref_impl(group, id, bench, true);
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
