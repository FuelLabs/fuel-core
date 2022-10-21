mod set;

use criterion::{
    black_box,
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
                // BatchSize::PerIteration,
                BatchSize::SmallInput,
            )
        },
    );
}

pub fn run_group_ref<I, M>(group: &mut BenchmarkGroup<M>, id: I, bench: VmBench)
where
    I: AsRef<str>,
    M: Measurement,
{
    let mut i = bench.prepare().expect("failed to prepare bench");
    group.bench_function::<_, _>(id.as_ref(), move |b| {
        b.iter(|| {
            let VmBenchPrepared { vm, instruction } = &mut i;

            match OpcodeRepr::from_u8(instruction.op()) {
                OpcodeRepr::CALL => {
                    let (_, ra, rb, rc, rd, _imm) = instruction.into_inner();
                    vm.prepare_call(ra, rb, rc, rd).unwrap();
                }
                _ => {
                    black_box(vm.instruction(black_box(*instruction)).unwrap());
                }
            }
        })
    });
}
fn vm(c: &mut Criterion) {
    alu::run(c);
    // blockchain::run(c);
    // crypto::run(c);
    // flow::run(c);
    // mem::run(c);
}

criterion_group!(benches, vm);
criterion_main!(benches);
