use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_vm::{prelude::{Interpreter, ConsensusParameters, GasCosts}, storage::MemoryStorage};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn bench(c: &mut Criterion) {    
    c.bench_function("mkvm", |b| {
        let vm_db = MemoryStorage::default();
        let params = ConsensusParameters::default();
        let gas_costs = GasCosts::default();

        b.iter(|| {
            let vm: Interpreter<MemoryStorage, ()> = Interpreter::with_storage(
                vm_db.clone(),
                params.clone(),
                gas_costs.clone(),
            );
            let _ = black_box(vm);
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);