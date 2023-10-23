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

    run(
        "flow/jmpb opcode",
        group,
        vec![op::noop(), op::jmpb(RegId::ZERO, 0)],
        vec![],
    );

    run(
        "flow/jmpf opcode",
        group,
        vec![op::jmpf(RegId::ZERO, 0), op::jmpb(RegId::ZERO, 0)],
        vec![],
    );

    run(
        "flow/jnzb opcode true",
        group,
        vec![
            op::movi(0x10, 1),
            op::noop(),
            op::jnzb(0x10, RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jnzb opcode false",
        group,
        vec![
            op::movi(0x10, 0),
            op::noop(),
            op::jnzb(0x10, RegId::ZERO, 0),
            op::jmpb(RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jnzf opcode true",
        group,
        vec![
            op::movi(0x10, 1),
            op::noop(),
            op::jnzf(0x10, RegId::ZERO, 1),
            op::ret(RegId::ZERO),
            op::jmpb(RegId::ZERO, 1),
        ],
        vec![],
    );

    run(
        "flow/jnzf opcode false",
        group,
        vec![
            op::movi(0x10, 0),
            op::noop(),
            op::jnzf(0x10, RegId::ZERO, 1),
            op::jmpb(RegId::ZERO, 0),
            op::noop(),
        ],
        vec![],
    );

    run(
        "flow/jneb opcode not equal",
        group,
        vec![
            op::movi(0x10, 1),
            op::movi(0x11, 0),
            op::noop(),
            op::jneb(0x10, 0x11, RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jneb opcode equal",
        group,
        vec![
            op::movi(0x10, 1),
            op::movi(0x11, 1),
            op::noop(),
            op::jneb(0x10, 0x11, RegId::ZERO, 0),
            op::jmpb(RegId::ZERO, 0),
        ],
        vec![],
    );

    run(
        "flow/jnef opcode not equal",
        group,
        vec![
            op::movi(0x10, 1),
            op::movi(0x11, 0),
            op::noop(),
            op::jnef(0x10, 0x11, RegId::ZERO, 1),
            op::ret(RegId::ZERO),
            op::jmpb(RegId::ZERO, 1),
        ],
        vec![],
    );

    run(
        "flow/jnef opcode equal",
        group,
        vec![
            op::movi(0x10, 1),
            op::movi(0x11, 1),
            op::noop(),
            op::jnef(0x10, 0x11, RegId::ZERO, 1),
            op::jmpb(RegId::ZERO, 0),
            op::noop(),
        ],
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
