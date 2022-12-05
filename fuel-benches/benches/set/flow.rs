use super::run_group_ref;

use criterion::Criterion;
use fuel_core_benches::*;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    run_group_ref(
        &mut c.benchmark_group("jmp"),
        "jmp",
        VmBench::new(Opcode::JMP(0x10)).with_prepare_script(vec![Opcode::MOVI(0x10, 10)]),
    );

    run_group_ref(
        &mut c.benchmark_group("ji"),
        "ji",
        VmBench::new(Opcode::JI(10)),
    );

    run_group_ref(
        &mut c.benchmark_group("jne"),
        "jne",
        VmBench::new(Opcode::JNE(REG_ZERO, REG_ONE, 0x10))
            .with_prepare_script(vec![Opcode::MOVI(0x10, 10)]),
    );

    run_group_ref(
        &mut c.benchmark_group("jnei"),
        "jnei",
        VmBench::new(Opcode::JNEI(REG_ZERO, REG_ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("jnzi"),
        "jnzi",
        VmBench::new(Opcode::JNZI(REG_ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("ret_script"),
        "ret (script)",
        VmBench::new(Opcode::RET(REG_ONE)),
    );

    run_group_ref(
        &mut c.benchmark_group("ret_contract"),
        "ret (contract)",
        VmBench::contract(rng, Opcode::RET(REG_ONE)).unwrap(),
    );
}
