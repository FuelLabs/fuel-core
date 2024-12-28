use crate::{
    utils::{
        arb_dependent_cost_values,
        set_full_word,
    },
    *,
};

// ALOC: Allocate memory
// CFE: Extend call frame
// CFEI: Extend call frame immediate
// CFS: Shrink call frame
// CFSI: Shrink call frame immediate
// LB: Load byte
// LW: Load word
// MCL: Memory clear
// MCLI: Memory clear immediate
// MCP: Memory copy
// MCPI: Memory copy immediate
// MEQ: Memory equality
// POPH: Pop a set of high registers from stack
// POPL: Pop a set of low registers from stack
// PSHH: Push a set of high registers to stack
// PSHL: Push a set of low registers to stack
// SB: Store byte
// SW: Store word
pub fn run_memory(group: &mut BenchmarkGroup<WallTime>) {
    // run(
    //     "memory/aloc opcode",
    //     group,
    //     [op::movi(0x10, 0), op::aloc(0x10), op::jmpb(RegId::ZERO, 0)].to_vec(),
    //     vec![],
    // );
    //
    // // Extend by 10 10 times and then shrink by 100 once. This is to hopefully allow the extend to be
    // // the dominant opcode
    // run(
    //     "memory/cfe opcode",
    //     group,
    //     vec![
    //         op::movi(0x10, 10),
    //         op::movi(0x11, 100),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfe(0x10),
    //         op::cfs(0x11),
    //         op::jmpb(RegId::ZERO, 10),
    //     ],
    //     vec![],
    // );
    //
    // // Extend by 10 10 times and then shrink by 100 once. This is to hopefully allow the extend to be
    // // the dominant opcode
    // run(
    //     "memory/cfei opcode",
    //     group,
    //     vec![
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfei(10),
    //         op::cfsi(100),
    //         op::jmpb(RegId::ZERO, 10),
    //     ],
    //     vec![],
    // );
    //
    // // Extend by 100 once and then shrink by 10 10 times. This is to hopefully allow the shrink to
    // // be the dominant opcode
    // run(
    //     "memory/cfs opcode",
    //     group,
    //     vec![
    //         op::movi(0x10, 100),
    //         op::movi(0x11, 10),
    //         op::cfe(0x10),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::cfs(0x11),
    //         op::jmpb(RegId::ZERO, 10),
    //     ],
    //     vec![],
    // );
    //
    // // Extend by 100 once and then shrink by 10 10 times. This is to hopefully allow the shrink to
    // // be the dominant opcode
    // run(
    //     "memory/cfsi opcode",
    //     group,
    //     vec![
    //         op::cfei(100),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::cfsi(10),
    //         op::jmpb(RegId::ZERO, 10),
    //     ],
    //     vec![],
    // );
    //
    // run(
    //     "memory/lb opcode",
    //     group,
    //     [op::lb(0x10, RegId::ONE, 10), op::jmpb(RegId::ZERO, 0)].to_vec(),
    //     vec![],
    // );
    //
    // run(
    //     "memory/lw opcode",
    //     group,
    //     [op::lw(0x10, RegId::ONE, 10), op::jmpb(RegId::ZERO, 0)].to_vec(),
    //     vec![],
    // );
    //
    // for i in arb_dependent_cost_values() {
    //     let id = format!("memory/mcl opcode {:?}", i);
    //     run(
    //         &id,
    //         group,
    //         vec![
    //             op::movi(0x11, i),
    //             op::aloc(0x11),
    //             op::move_(0x10, RegId::HP),
    //             op::mcl(0x10, 0x11),
    //             op::jmpb(RegId::ZERO, 0),
    //         ],
    //         vec![],
    //     );
    // }
    //
    // for i in arb_dependent_cost_values() {
    //     let id = format!("memory/mcli opcode {:?}", i);
    //     run(
    //         &id,
    //         group,
    //         vec![
    //             op::movi(0x11, i),
    //             op::aloc(0x11),
    //             op::move_(0x10, RegId::HP),
    //             op::mcli(0x10, i),
    //             op::jmpb(RegId::ZERO, 0),
    //         ],
    //         vec![],
    //     );
    // }
    //
    // for i in arb_dependent_cost_values() {
    //     let id = format!("memory/mcp opcode {:?}", i);
    //     run(
    //         &id,
    //         group,
    //         vec![
    //             op::movi(0x11, i),
    //             op::aloc(0x11),
    //             op::move_(0x10, RegId::HP),
    //             op::mcp(0x10, RegId::ZERO, 0x11),
    //             op::jmpb(RegId::ZERO, 0),
    //         ],
    //         vec![],
    //     );
    // }
    //
    // let valid_values: Vec<_> = arb_dependent_cost_values()
    //     .iter()
    //     .copied()
    //     .take_while(|p| *p < (1 << 12)) // 12 bits
    //     .collect();
    // for val in valid_values {
    //     let id = format!("memory/mcpi opcode {:?}", val);
    //     let val_as_u16 = (val).try_into().unwrap();
    //     run(
    //         &id,
    //         group,
    //         vec![
    //             op::movi(0x11, val),
    //             op::aloc(0x11),
    //             op::move_(0x10, RegId::HP),
    //             op::mcpi(0x10, RegId::ZERO, val_as_u16),
    //             op::jmpb(RegId::ZERO, 0),
    //         ],
    //         vec![],
    //     );
    // }

    for i in arb_dependent_cost_values() {
        let id = format!("memory/meq opcode {:?}", i);
        let mut script = set_full_word(0x13, i as u64);
        script.extend(vec![
            op::meq(0x10, RegId::ZERO, RegId::ZERO, 0x13),
            op::jmpb(RegId::ZERO, 0),
        ]);
        run(&id, group, script, vec![]);
    }

    // let full_mask = (1 << 24) - 1;
    //
    // // Assumes that `pshh` has a correct cost
    // run(
    //     "memory/poph opcode",
    //     group,
    //     vec![
    //         op::pshh(full_mask),
    //         op::poph(full_mask),
    //         op::jmpb(RegId::ZERO, 1),
    //     ],
    //     vec![],
    // );
    //
    // // Assumes that `pshl` has a correct cost
    // run(
    //     "memory/popl opcode",
    //     group,
    //     vec![
    //         op::pshl(full_mask),
    //         op::popl(full_mask),
    //         op::jmpb(RegId::ZERO, 1),
    //     ],
    //     vec![],
    // );
    //
    // run(
    //     "memory/pshh opcode",
    //     group,
    //     vec![op::pshh(full_mask), op::jmpb(RegId::ZERO, 0)],
    //     vec![],
    // );
    //
    // run(
    //     "memory/pshl opcode",
    //     group,
    //     vec![op::pshl(full_mask), op::jmpb(RegId::ZERO, 0)],
    //     vec![],
    // );
    //
    // run(
    //     "memory/sb opcode",
    //     group,
    //     vec![
    //         op::aloc(RegId::ONE),
    //         op::move_(0x10, RegId::HP),
    //         op::movi(0x11, 50),
    //         op::sb(0x10, 0x11, 0),
    //         op::jmpb(RegId::ZERO, 0),
    //     ],
    //     vec![],
    // );
    //
    // run(
    //     "memory/sw opcode",
    //     group,
    //     vec![
    //         op::movi(0x10, 8),
    //         op::aloc(0x10),
    //         op::move_(0x10, RegId::HP),
    //         op::movi(0x11, 50),
    //         op::sw(0x10, 0x11, 0),
    //         op::jmpb(RegId::ZERO, 0),
    //     ],
    //     vec![],
    // );
}
