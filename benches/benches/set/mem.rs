use std::iter::successors;

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;
use fuel_core_types::fuel_asm::*;

/// Set a register `r` to a Word-sized number value using left-shifts
fn set_full_word(r: RegisterId, v: Word) -> Vec<Instruction> {
    let r = u8::try_from(r).unwrap();
    let mut ops = vec![op::movi(r, 0)];
    for byte in v.to_be_bytes() {
        ops.push(op::ori(r, r, byte as u16));
        ops.push(op::slli(r, r, 8));
    }
    ops.pop().unwrap(); // Remove last shift
    ops
}

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
            op::move_(0x10, RegId::HP),
            op::movi(0x11, 50),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("sw"),
        "sw",
        VmBench::new(op::sw(0x10, 0x11, 0)).with_prepare_script(vec![
            op::movi(0x10, 8),
            op::aloc(0x10),
            op::move_(0x10, RegId::HP),
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
                op::move_(0x10, RegId::HP),
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
                op::move_(0x10, RegId::HP),
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
                op::move_(0x10, RegId::HP),
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
            op::move_(0x10, RegId::HP),
        ]),
    );

    let mut mem_meq = c.benchmark_group("meq");
    for i in &linear {
        mem_meq.throughput(Throughput::Bytes(*i as u64));

        let mut prepare_script = vec![op::movi(0x11, 0)];
        prepare_script.extend(set_full_word(0x12, (i * 3) as u64));
        prepare_script.extend(set_full_word(0x13, (*i) as u64));

        run_group_ref(
            &mut mem_meq,
            format!("{i}"),
            VmBench::new(op::meq(0x10, 0x11, 0x12, 0x13))
                .with_prepare_script(prepare_script),
        );
    }
    mem_meq.finish();
}
