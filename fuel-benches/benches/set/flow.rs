use super::run_group;

use criterion::Criterion;
use fuel_core_benches::*;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut group = c.benchmark_group("flow");

    run_group(
        &mut group,
        "jmp",
        VmBench::new(Opcode::JMP(0x10)).with_prepare_script(vec![Opcode::MOVI(0x10, 10)]),
    );

    run_group(&mut group, "ji", VmBench::new(Opcode::JI(10)));

    run_group(
        &mut group,
        "jne",
        VmBench::new(Opcode::JNE(REG_ZERO, REG_ONE, 0x10))
            .with_prepare_script(vec![Opcode::MOVI(0x10, 10)]),
    );

    run_group(
        &mut group,
        "jnei",
        VmBench::new(Opcode::JNEI(REG_ZERO, REG_ONE, 10)),
    );

    run_group(&mut group, "jnzi", VmBench::new(Opcode::JNZI(REG_ONE, 10)));

    run_group(
        &mut group,
        "ret (script)",
        VmBench::new(Opcode::RET(REG_ONE)),
    );

    run_group(
        &mut group,
        "ret (contract)",
        VmBench::contract(rng, Opcode::RET(REG_ONE)).unwrap(),
    );

    group.finish();
}
