use crate::*;

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
    run(
        "flow/jmp opcode",
        group,
        vec![op::movi(0x10, 0), op::jmp(0x10)],
        vec![],
    );

    run(
        "flow/ji opcode",
        group,
        vec![op::ji(0), op::jmpb(RegId::ZERO, 0)],
        vec![],
    );

    run(
        "flow/jne opcode",
        group,
        vec![
            op::movi(0x10, 0),
            op::jne(RegId::ZERO, RegId::ONE, 0x10),
            op::jmpb(RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jnei opcode",
        group,
        vec![
            op::jnei(RegId::ZERO, RegId::ONE, 0),
            op::jmpb(RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jnzi opcode",
        group,
        vec![op::jnzi(RegId::ONE, 0), op::jmpb(RegId::ZERO, 0)],
        vec![],
    );

    // Don't know how to test "returning" op codes
    // run(
    //     "flow/ret_script opcode",
    //     group,
    //     vec![op::ret(RegId::ONE), op::jmpb(RegId::ZERO, 0)].to_vec(),
    //     vec![],
    // );
    //
    // run(
    //     "flow/ret_contract opcode",
    //     group,
    //     vec![op::ret(RegId::ONE), op::jmpb(RegId::ZERO, 0)].to_vec(),
    //     vec![],
    // );
}
