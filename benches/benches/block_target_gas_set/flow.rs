use crate::*;

use rand::{
    rngs::StdRng,
    SeedableRng,
};

// JMP: Jump
// JI: Jump immediate
// JNE: Jump if not equal
// JNEI: Jump if not equal immediate
// JNZI: Jump if not zero immediate
// JMPB: Jump relative backwards
// JMPF: Jump relative forwards
// JNZB: Jump if not zero relative backwards
// JNZF: Jump if not zero relative forwards
// JNEB: Jump if not equal relative backwards
// JNEF: Jump if not equal relative forwards
// RET: Return from context
pub fn run_flow(group: &mut BenchmarkGroup<WallTime>) {
    let rng = &mut StdRng::seed_from_u64(2322u64);
    //     let rng = &mut StdRng::seed_from_u64(2322u64);
    //
    //     let mut linear = vec![1, 10, 100, 1000, 10_000];
    //     let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
    //         .take(5)
    //         .map(|f| f as u32)
    //         .collect::<Vec<_>>();
    //     l.sort_unstable();
    //     linear.extend(l);
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("jmp"),
    //         "jmp",
    //         VmBench::new(op::jmp(0x10)).with_prepare_script(vec![op::movi(0x10, 10)]),
    //     );
    //
    //     run_group_ref(&mut c.benchmark_group("ji"), "ji", VmBench::new(op::ji(10)));
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("jne"),
    //         "jne",
    //         VmBench::new(op::jne(RegId::ZERO, RegId::ONE, 0x10))
    //             .with_prepare_script(vec![op::movi(0x10, 10)]),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("jnei"),
    //         "jnei",
    //         VmBench::new(op::jnei(RegId::ZERO, RegId::ONE, 10)),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("jnzi"),
    //         "jnzi",
    //         VmBench::new(op::jnzi(RegId::ONE, 10)),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("ret_script"),
    //         "ret_script",
    //         VmBench::new(op::ret(RegId::ONE)),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("ret_contract"),
    //         "ret_contract",
    //         VmBench::contract(rng, op::ret(RegId::ONE)).unwrap(),
    //     );
    //
    //     let mut retd_contract = c.benchmark_group("retd_contract");
    //     for i in &linear {
    //         retd_contract.throughput(Throughput::Bytes(*i as u64));
    //         run_group_ref(
    //             &mut retd_contract,
    //             format!("{i}"),
    //             VmBench::contract(rng, op::retd(RegId::ONE, 0x10))
    //                 .unwrap()
    //                 .with_post_call(vec![op::movi(0x10, *i)]),
    //         );
    //     }
    //     retd_contract.finish();
    //
    //     let mut retd_script = c.benchmark_group("retd_script");
    //     for i in &linear {
    //         retd_script.throughput(Throughput::Bytes(*i as u64));
    //         run_group_ref(
    //             &mut retd_script,
    //             format!("{i}"),
    //             VmBench::contract(rng, op::retd(RegId::ONE, 0x10))
    //                 .unwrap()
    //                 .with_post_call(vec![op::movi(0x10, *i)]),
    //         );
    //     }
    //     retd_script.finish();
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("rvrt_script"),
    //         "rvrt_script",
    //         VmBench::new(op::rvrt(RegId::ONE)),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("rvrt_contract"),
    //         "rvrt_contract",
    //         VmBench::contract(rng, op::ret(RegId::ONE)).unwrap(),
    //     );
    //
    //     run_group_ref(
    //         &mut c.benchmark_group("log"),
    //         "log",
    //         VmBench::new(op::log(0x10, 0x11, 0x12, 0x13)),
    //     );
    //
    //     let mut logd = c.benchmark_group("logd");
    //     for i in &linear {
    //         logd.throughput(Throughput::Bytes(*i as u64));
    //         run_group_ref(
    //             &mut logd,
    //             format!("{i}"),
    //             VmBench::new(op::logd(0x10, 0x11, RegId::ZERO, 0x13))
    //                 .with_prepare_script(vec![op::movi(0x13, *i)]),
    //         );
    //     }
    //     logd.finish();
    run(
        "flow/jmp opcode",
        group,
        vec![op::movi(0x10, 0), op::jmp(0x10)].to_vec(),
        vec![],
    );

    run(
        "flow/ji opcode",
        group,
        vec![op::ji(0), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "flow/jne opcode",
        group,
        vec![
            op::movi(0x10, 0),
            op::jne(RegId::ZERO, RegId::ONE, 0x10),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "flow/jnei opcode",
        group,
        vec![
            op::jnei(RegId::ZERO, RegId::ONE, 0),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "flow/jnzi opcode",
        group,
        vec![op::jnzi(RegId::ONE, 0), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "flow/ret_script opcode",
        group,
        vec![op::ret(RegId::ONE), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "flow/ret_contract opcode",
        group,
        vec![op::ret(RegId::ONE), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );
}
