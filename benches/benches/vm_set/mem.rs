use super::run_group_ref;

use crate::utils::{arb_dependent_cost_values, linear_short, set_full_word};

use criterion::{Criterion, Throughput};
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_vm::{consts::MEM_SIZE, interpreter::MemoryInstance},
};

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

    let linear = arb_dependent_cost_values();
    let linear_short = linear_short();

    // cfe
    {
        let mut cfe = c.benchmark_group("cfe");

        let mut cfe_linear = linear_short.clone();
        cfe_linear.push(1_000_000);
        cfe_linear.push(10_000_000);
        cfe_linear.push(30_000_000);
        cfe_linear.push(60_000_000);
        for i in cfe_linear {
            let prepare_script = set_full_word(0x10, i);
            let memory = MemoryInstance::from(vec![123; MEM_SIZE]);
            let bench = VmBench::new(op::cfe(0x10))
                .with_prepare_script(prepare_script)
                .with_memory(memory);
            cfe.throughput(Throughput::Bytes(i));
            run_group_ref(&mut cfe, format!("{i}"), bench);
        }

        cfe.finish();
    }

    // cfei
    {
        let mut cfei = c.benchmark_group("cfei");

        let mut cfei_linear = linear_short.clone();
        cfei_linear.push(1_000_000);
        cfei_linear.push(10_000_000);
        for i in cfei_linear {
            let memory = MemoryInstance::from(vec![123; MEM_SIZE]);
            let bench = VmBench::new(op::cfei(i as u32)).with_memory(memory);
            cfei.throughput(Throughput::Bytes(i));
            run_group_ref(&mut cfei, format!("{i}"), bench);
        }

        cfei.finish();
    }

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
                op::cfe(0x11),
                op::move_(0x10, RegId::HP),
            ]),
        );
    }
    mem_mcp.finish();

    let mut mem_mcpi = c.benchmark_group("mcpi");

    let mut imm12_linear: Vec<_> = linear
        .iter()
        .copied()
        .take_while(|p| *p < (1 << 12))
        .collect();
    imm12_linear.push((1 << 12) - 1);
    for i in &imm12_linear {
        let i_as_u16: u16 = (*i).try_into().unwrap();
        mem_mcpi.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut mem_mcpi,
            format!("{i}"),
            VmBench::new(op::mcpi(0x10, RegId::ZERO, i_as_u16)).with_prepare_script(
                vec![
                    op::movi(0x11, *i),
                    op::aloc(0x11),
                    op::cfe(0x11),
                    op::move_(0x10, RegId::HP),
                ],
            ),
        );
    }
    mem_mcpi.finish();

    let mut mem_meq = c.benchmark_group("meq");
    for i in &linear {
        let i = *i as u64;
        mem_meq.throughput(Throughput::Bytes(i));

        let mut prepare_script =
            vec![op::move_(0x11, RegId::ZERO), op::move_(0x12, RegId::ZERO)];
        prepare_script.extend(set_full_word(0x13, i));
        prepare_script.extend(vec![op::cfe(0x13)]);

        run_group_ref(
            &mut mem_meq,
            format!("{i}"),
            VmBench::new(op::meq(0x10, 0x11, 0x12, 0x13))
                .with_prepare_script(prepare_script),
        );
    }
    mem_meq.finish();

    let full_mask = (1 << 24) - 1;

    // poph
    let prepare_script = vec![op::pshh(full_mask)];
    run_group_ref(
        &mut c.benchmark_group("poph"),
        "poph",
        VmBench::new(op::poph(full_mask)).with_prepare_script(prepare_script),
    );

    // popl
    let prepare_script = vec![op::pshl(full_mask)];
    run_group_ref(
        &mut c.benchmark_group("popl"),
        "popl",
        VmBench::new(op::popl(full_mask)).with_prepare_script(prepare_script),
    );

    // pshh
    run_group_ref(
        &mut c.benchmark_group("pshh"),
        "pshh",
        VmBench::new(op::pshh(full_mask))
            .with_prepare_script(vec![op::pshh(full_mask); 10000]),
    );

    // pshl
    run_group_ref(
        &mut c.benchmark_group("pshl"),
        "pshl",
        VmBench::new(op::pshl(full_mask))
            .with_prepare_script(vec![op::pshl(full_mask); 10000]),
    );
}
