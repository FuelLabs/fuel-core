use super::run_group_ref;

use crate::utils::{
    arb_dependent_cost_values,
    make_receipts,
};
use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::VmBench;
use fuel_core_types::fuel_asm::*;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let linear = arb_dependent_cost_values();
    let receipts_ctx = make_receipts(rng);

    run_group_ref(
        &mut c.benchmark_group("jmp"),
        "jmp",
        VmBench::new(op::jmp(0x10)).with_prepare_script(vec![op::movi(0x10, 10)]),
    );

    run_group_ref(&mut c.benchmark_group("ji"), "ji", VmBench::new(op::ji(10)));

    run_group_ref(
        &mut c.benchmark_group("jne"),
        "jne",
        VmBench::new(op::jne(RegId::ZERO, RegId::ONE, 0x10))
            .with_prepare_script(vec![op::movi(0x10, 10)]),
    );

    run_group_ref(
        &mut c.benchmark_group("jnei"),
        "jnei",
        VmBench::new(op::jnei(RegId::ZERO, RegId::ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("jnzi"),
        "jnzi",
        VmBench::new(op::jnzi(RegId::ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("ret_script"),
        "ret_script",
        VmBench::new(op::ret(RegId::ONE)).with_call_receipts(receipts_ctx.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("ret_contract"),
        "ret_contract",
        VmBench::contract(rng, op::ret(RegId::ONE))
            .unwrap()
            .with_call_receipts(receipts_ctx.clone()),
    );

    let mut retd_contract = c.benchmark_group("retd_contract");
    for i in &linear {
        retd_contract.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut retd_contract,
            format!("{i}"),
            VmBench::contract(rng, op::retd(RegId::ONE, 0x10))
                .unwrap()
                .with_post_call(vec![op::movi(0x10, *i), op::cfe(0x10)])
                .with_call_receipts(receipts_ctx.clone()),
        );
    }
    retd_contract.finish();

    let mut retd_script = c.benchmark_group("retd_script");
    for i in &linear {
        retd_script.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut retd_script,
            format!("{i}"),
            VmBench::contract(rng, op::retd(RegId::ONE, 0x10))
                .unwrap()
                .with_post_call(vec![op::movi(0x10, *i), op::cfe(0x10)])
                .with_call_receipts(receipts_ctx.clone()),
        );
    }
    retd_script.finish();

    run_group_ref(
        &mut c.benchmark_group("rvrt_script"),
        "rvrt_script",
        VmBench::new(op::rvrt(RegId::ONE)).with_call_receipts(receipts_ctx.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("rvrt_contract"),
        "rvrt_contract",
        VmBench::contract(rng, op::ret(RegId::ONE))
            .unwrap()
            .with_call_receipts(receipts_ctx.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("log"),
        "log",
        VmBench::new(op::log(0x10, 0x11, 0x12, 0x13))
            .with_call_receipts(receipts_ctx.clone()),
    );

    let mut logd = c.benchmark_group("logd");
    for i in &linear {
        logd.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut logd,
            format!("{i}"),
            VmBench::new(op::logd(0x10, 0x11, RegId::ZERO, 0x13))
                .with_prepare_script(vec![op::movi(0x13, *i), op::cfe(0x13)])
                .with_call_receipts(receipts_ctx.clone()),
        );
    }
    logd.finish();
}
