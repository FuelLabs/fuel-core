use criterion::{
    criterion_group,
    criterion_main,
    measurement::Measurement,
    BatchSize,
    BenchmarkGroup,
    Criterion,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

use fuel_core_benches::*;

fn run_group<I, M>(group: &mut BenchmarkGroup<M>, id: I, bench: VmBench)
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
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut group = c.benchmark_group("alu");

    run_group(
        &mut group,
        "add",
        VmBench::new(Opcode::ADD(0x10, 0x01, 0x01)),
    );

    run_group(
        &mut group,
        "mlog",
        VmBench::new(Opcode::MLOG(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(&mut group, "noop", VmBench::new(Opcode::NOOP));

    group.finish();

    let mut group = c.benchmark_group("contract");

    run_group(&mut group, "bhei", VmBench::new(Opcode::BHEI(0x10)));

    run_group(
        &mut group,
        "bhsh",
        VmBench::new(Opcode::BHSH(0x10, REG_ZERO)).with_prepare_script(vec![
            Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
        ]),
    );

    run_group(
        &mut group,
        "call",
        VmBench::contract(rng, Opcode::RET(REG_ONE)).unwrap(),
    );

    run_group(
        &mut group,
        "sww",
        VmBench::contract(rng, Opcode::SWW(REG_ZERO, REG_ONE)).unwrap(),
    );

    group.finish();

    let mut group = c.benchmark_group("crypto");

    let secret = SecretKey::random(rng);
    let message = Message::new(b"foo");
    let signature = Signature::sign(&secret, &message);

    run_group(
        &mut group,
        "ecr",
        VmBench::new(Opcode::ECR(0x11, 0x20, 0x21))
            .with_prepare_script(vec![
                Opcode::gtf(0x20, 0x00, GTFArgs::ScriptData),
                Opcode::ADDI(0x21, 0x20, signature.as_ref().len() as Immediate12),
                Opcode::ADDI(0x22, 0x21, message.as_ref().len() as Immediate12),
                Opcode::MOVI(0x10, PublicKey::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x11, REG_HP, 1),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group(
        &mut group,
        "s256",
        VmBench::new(Opcode::S256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x10, REG_HP, 1),
                Opcode::MOVI(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group(
        &mut group,
        "k256",
        VmBench::new(Opcode::K256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x10, REG_HP, 1),
                Opcode::MOVI(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );
}

criterion_group!(benches, vm);
criterion_main!(benches);
