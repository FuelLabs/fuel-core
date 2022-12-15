mod set;

use criterion::{
    criterion_group,
    criterion_main,
    measurement::Measurement,
    BatchSize,
    BenchmarkGroup,
    Criterion,
};

use fuel_core_benches::*;
use set::*;

pub fn run_group<I, M>(group: &mut BenchmarkGroup<M>, id: I, bench: VmBench)
where
    I: AsRef<str>,
    M: Measurement,
{
    group.bench_with_input::<_, _, VmBenchPrepared>(
        id.as_ref(),
        &bench.prepare().expect("failed to prepare bench"),
        |b, i| {
            b.iter_batched(
                || i.clone(),
                |i| i.run().expect("failed to execute tx"),
                BatchSize::PerIteration,
            )
        },
    );
}

fn vm(c: &mut Criterion) {
    alu::run(c);
    blockchain::run(c);
    crypto::run(c);
    flow::run(c);
    mem::run(c);
}

criterion_group!(benches, vm);
criterion_main!(benches);
