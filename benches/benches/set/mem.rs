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

pub fn run(c: &mut Criterion) {
    run_group_ref(
        &mut c.benchmark_group("lb"),
        "lb",
        VmBench::new(Opcode::LB(0x10, REG_ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("lw"),
        "lw",
        VmBench::new(Opcode::LW(0x10, REG_ONE, 10)),
    );

    run_group_ref(
        &mut c.benchmark_group("sb"),
        "sb",
        VmBench::new(Opcode::SB(0x10, 0x11, 0)).with_prepare_script(vec![
            Opcode::ALOC(REG_ONE),
            Opcode::ADDI(0x10, REG_HP, 1),
            Opcode::MOVI(0x11, 50),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("sw"),
        "sw",
        VmBench::new(Opcode::SW(0x10, 0x11, 0)).with_prepare_script(vec![
            Opcode::MOVI(0x10, 8),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
            Opcode::MOVI(0x11, 50),
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
        VmBench::new(Opcode::CFEI(1)),
    );

    let mut mem_mcl = c.benchmark_group("mcl");
    for i in &linear {
        mem_mcl.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcl,
            format!("{i}"),
            VmBench::new(Opcode::MCL(0x10, 0x11)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
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
            VmBench::new(Opcode::MCLI(0x10, *i)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
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
            VmBench::new(Opcode::MCP(0x10, REG_ZERO, 0x11)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
            ]),
        );
    }
    mem_mcp.finish();

    run_group_ref(
        &mut c.benchmark_group("mcpi"),
        "mcpi",
        VmBench::new(Opcode::MCPI(0x10, REG_ZERO, 4000 as Immediate12))
            .with_prepare_script(vec![
                Opcode::MOVI(0x11, 4000),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
            ]),
    );

    let mut mem_meq = c.benchmark_group("meq");
    for i in &linear {
        mem_meq.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_meq,
            format!("{i}"),
            VmBench::new(Opcode::MEQ(0x10, 0x11, 0x12, 0x13)).with_prepare_script(vec![
                Opcode::MOVI(0x11, 0),
                Opcode::MOVI(0x12, i * 3),
                Opcode::MOVI(0x13, *i),
            ]),
        );
    }
    mem_meq.finish();
}
