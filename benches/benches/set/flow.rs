use std::iter::successors;

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_vm::consts::*,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut linear = vec![1, 10, 100, 1000, 10_000];
    let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
        .take(5)
        .map(|f| f as u32)
        .collect::<Vec<_>>();
    l.sort_unstable();
    linear.extend(l);

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
        "ret_script",
        VmBench::new(Opcode::RET(REG_ONE)),
    );

    run_group_ref(
        &mut c.benchmark_group("ret_contract"),
        "ret_contract",
        VmBench::contract(rng, Opcode::RET(REG_ONE)).unwrap(),
    );

    let mut retd_contract = c.benchmark_group("retd_contract");
    for i in &linear {
        retd_contract.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut retd_contract,
            format!("{}", i),
            VmBench::contract(rng, Opcode::RETD(REG_ONE, 0x10))
                .unwrap()
                .with_post_call(vec![Opcode::MOVI(0x10, *i)]),
        );
    }
    retd_contract.finish();

    let mut retd_script = c.benchmark_group("retd_script");
    for i in &linear {
        retd_script.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut retd_script,
            format!("{}", i),
            VmBench::contract(rng, Opcode::RETD(REG_ONE, 0x10))
                .unwrap()
                .with_post_call(vec![Opcode::MOVI(0x10, *i)]),
        );
    }
    retd_script.finish();

    run_group_ref(
        &mut c.benchmark_group("rvrt_script"),
        "rvrt_script",
        VmBench::new(Opcode::RVRT(REG_ONE)),
    );

    run_group_ref(
        &mut c.benchmark_group("rvrt_contract"),
        "rvrt_contract",
        VmBench::contract(rng, Opcode::RET(REG_ONE)).unwrap(),
    );

    run_group_ref(
        &mut c.benchmark_group("log"),
        "log",
        VmBench::new(Opcode::LOG(0x10, 0x11, 0x12, 0x13)),
    );

    let mut logd = c.benchmark_group("logd");
    for i in &linear {
        logd.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut logd,
            format!("{}", i),
            VmBench::new(Opcode::LOGD(0x10, 0x11, REG_ZERO, 0x13))
                .with_prepare_script(vec![Opcode::MOVI(0x13, *i)]),
        );
    }
    logd.finish();
}
