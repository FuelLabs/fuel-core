use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;

pub fn run(c: &mut Criterion) {
    let mut mem = c.benchmark_group("mem");

    run_group_ref(
        &mut mem,
        "aloc",
        VmBench::new(Opcode::ALOC(0x10)).with_prepare_script(vec![Opcode::MOVI(0x10, 1)]),
    );

    run_group_ref(&mut mem, "lb", VmBench::new(Opcode::LB(0x10, REG_ONE, 10)));

    run_group_ref(&mut mem, "lw", VmBench::new(Opcode::LW(0x10, REG_ONE, 10)));

    run_group_ref(
        &mut mem,
        "sb",
        VmBench::new(Opcode::SB(0x10, 0x11, 0)).with_prepare_script(vec![
            Opcode::ALOC(REG_ONE),
            Opcode::ADDI(0x10, REG_HP, 1),
            Opcode::MOVI(0x11, 50),
        ]),
    );

    run_group_ref(
        &mut mem,
        "sw",
        VmBench::new(Opcode::SW(0x10, 0x11, 0)).with_prepare_script(vec![
            Opcode::MOVI(0x10, 8),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
            Opcode::MOVI(0x11, 50),
        ]),
    );

    mem.finish();

    let linear = vec![1, 10, 100, 1_000, 10_000, 100_000];
    // let linear = vec![1, 10, 100, 1_000, 10_000];
    // let linear = vec![1, 10, 25, 50, 75, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1_000];

    let mut mem_cfei = c.benchmark_group("mem/cfei");
    for i in &linear {
        mem_cfei.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_cfei,
            format!("{}", i),
            VmBench::new(Opcode::CFEI(*i)).with_cleanup(vec![Opcode::CFSI(*i)]),
        );
    }
    mem_cfei.finish();

    let mut mem_mcl = c.benchmark_group("mem/mcl");
    for i in &linear {
        mem_mcl.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcl,
            format!("{}", i),
            VmBench::new(Opcode::MCL(0x10, 0x11)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
            ]),
        );
    }
    mem_mcl.finish();

    let mut mem_mcli = c.benchmark_group("mem/mcli");
    for i in &linear {
        mem_mcli.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcli,
            format!("{}", i),
            VmBench::new(Opcode::MCLI(0x10, *i)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
            ]),
        );
    }
    mem_mcli.finish();

    let mut mem_mcp = c.benchmark_group("mem/mcp");
    for i in &linear {
        mem_mcp.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcp,
            format!("{}", i),
            VmBench::new(Opcode::MCP(0x10, REG_ZERO, 0x11)).with_prepare_script(vec![
                Opcode::MOVI(0x11, *i),
                Opcode::ALOC(0x11),
                Opcode::ADDI(0x10, REG_HP, 1),
            ]),
        );
    }
    mem_mcp.finish();

    let mut mem_mcpi = c.benchmark_group("mem/mcpi");
    for i in &linear {
        mem_mcpi.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcpi,
            format!("{}", i),
            VmBench::new(Opcode::MCPI(0x10, REG_ZERO, *i as Immediate12))
                .with_prepare_script(vec![
                    Opcode::MOVI(0x11, *i),
                    Opcode::ALOC(0x11),
                    Opcode::ADDI(0x10, REG_HP, 1),
                ]),
        );
    }
    mem_mcpi.finish();

    let mut mem_meq = c.benchmark_group("mem/meq");
    for i in &linear {
        mem_meq.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_meq,
            format!("{}", i),
            VmBench::new(Opcode::MEQ(0x10, 0x11, 0x12, 0x13)).with_prepare_script(vec![
                Opcode::MOVI(0x11, 0),
                Opcode::MOVI(0x12, i * 3),
                Opcode::MOVI(0x13, *i),
            ]),
        );
    }
    mem_meq.finish();
}
