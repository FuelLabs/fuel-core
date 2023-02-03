use std::iter::successors;

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;
use fuel_core_types::fuel_asm::*;

pub fn run(c: &mut Criterion) {
    run_group_ref(
        &mut c.benchmark_group("lb"),
        "lb",
        VmBench::new(op::lb(0x10, RegId::ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("lw"),
        "lw",
        VmBench::new(op::lw(0x10, RegId::ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("sb"),
        "sb",
        VmBench::new(op::sb(0x10, 0x11, 0)).with_prepare_script(vec![
            op::aloc(RegId::ONE),
            op::addi(0x10, RegId::HP, 1),
            op::movi(0x11, 50),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("sw"),
        "sw",
        VmBench::new(op::sw(0x10, 0x11, 0)).with_prepare_script(vec![
            op::movi(0x10, 8),
            op::aloc(0x10),
            op::addi(0x10, RegId::HP, 1),
            op::movi(0x11, 50),
        ]),
    );

    let mut linear = vec![1, 10, 100, 1000, 10_000];
    let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
        .take(5)
        .map(|f| f as u32)
        .collect::<Vec<_>>();
    l.sort_unstable();
    linear.extend(l);

    run_group_ref(
        &mut c.benchmark_group("cfei"),
        "cfei",
        VmBench::new(op::cfei(1)),
    );

    let mut mem_mcl = c.benchmark_group("mcl");
    for i in &linear {
        mem_mcl.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcl,
            format!("{i}"),
            VmBench::new(op::mcl(0x10, 0x11)).with_prepare_script(vec![
                op::movi(0x11, *i),
                op::aloc(0x11),
                op::addi(0x10, RegId::HP, 1),
            ]),
        );
    }
    mem_mcl.finish();

    let mut mem_mcli = c.benchmark_group("mcli");
    for i in &linear {
        mem_mcli.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcli,
            format!("{i}"),
            VmBench::new(op::mcli(0x10, *i)).with_prepare_script(vec![
                op::movi(0x11, *i),
                op::aloc(0x11),
                op::addi(0x10, RegId::HP, 1),
            ]),
        );
    }
    mem_mcli.finish();

    let mut mem_mcp = c.benchmark_group("mcp");
    for i in &linear {
        mem_mcp.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcp,
            format!("{i}"),
            VmBench::new(op::mcp(0x10, RegId::ZERO, 0x11)).with_prepare_script(vec![
                op::movi(0x11, *i),
                op::aloc(0x11),
                op::addi(0x10, RegId::HP, 1),
            ]),
        );
    }
    mem_mcp.finish();

    run_group_ref(
        &mut c.benchmark_group("mcpi"),
        "mcpi",
        VmBench::new(op::mcpi(0x10, RegId::ZERO, 4000)).with_prepare_script(vec![
            op::movi(0x11, 4000),
            op::aloc(0x11),
            op::addi(0x10, RegId::HP, 1),
        ]),
    );

    let mut mem_meq = c.benchmark_group("meq");
    for i in &linear {
        mem_meq.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_meq,
            format!("{i}"),
            VmBench::new(op::meq(0x10, 0x11, 0x12, 0x13)).with_prepare_script(vec![
                op::movi(0x11, 0),
                op::movi(0x12, i * 3),
                op::movi(0x13, *i),
            ]),
        );
    }
    mem_meq.finish();
}
