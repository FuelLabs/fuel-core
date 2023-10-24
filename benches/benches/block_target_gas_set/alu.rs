use crate::*;

pub fn run_alu(group: &mut BenchmarkGroup<WallTime>) {
    run(
        "alu/add opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::add(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/addi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::addi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/and opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::and(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/andi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::andi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/div opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::div(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/divi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::divi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/eq opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::eq(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/exp opcode",
        group,
        [
            op::movi(0x11, 23),
            op::movi(0x12, 11),
            op::exp(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/expi opcode",
        group,
        [
            op::movi(0x11, 23),
            op::expi(0x10, 0x11, 11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/gt opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::gt(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/lt opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::lt(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/mlog opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mlog(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/mod opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mod_(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/modi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::modi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/move opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::move_(0x10, 0x11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/movi opcode",
        group,
        [op::movi(0x10, 27), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "alu/mroo opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mroo(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/mul opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mul(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/muli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::muli(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/mldv opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::movi(0x13, 2),
            op::mldv(0x10, 0x11, 0x12, 0x13),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/noop opcode",
        group,
        [op::noop(), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "alu/not opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::not(0x10, 0x11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/or opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::or(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/ori opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::ori(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/sll opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::sll(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/slli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::slli(0x10, 0x11, 3),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/srl opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::srl(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/srli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::srli(0x10, 0x11, 3),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/sub opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::sub(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/subi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::subi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/xor opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::xor(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "alu/xori opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::xori(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u128(0x10, 0));
    wideint_prepare.extend(make_u128(0x11, u128::MAX));
    wideint_prepare.extend(make_u128(0x12, u128::MAX / 2 + 1));
    wideint_prepare.extend(make_u128(0x13, u128::MAX - 158)); // prime
    wideint_prepare.extend(make_u128(0x14, u64::MAX.into()));

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wdcm opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wdop opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wdml opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wddv_args(0x10, 0x12, 0x13, DivArgs { indirect_rhs: true }),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wddv opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdmd(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wdmd opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdam(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wdam opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdmm(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wdmm opcode", group, instructions, vec![]);

    // Wideint operations: 256 bit
    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u256(0x10, U256::ZERO));
    wideint_prepare.extend(make_u256(0x11, U256::MAX));
    wideint_prepare.extend(make_u256(0x12, U256::MAX / 2 + 1));
    wideint_prepare.extend(make_u256(0x13, U256::MAX - 188)); // prime
    wideint_prepare.extend(make_u256(0x14, u128::MAX.into()));

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wqcm opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wqop opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wqml opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqdv_args(0x10, 0x12, 0x13, DivArgs { indirect_rhs: true }),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("alu/wqdv opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqmd(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wqmd opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqam(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wqam opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqmm(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("alu/wqmm opcode", group, instructions, vec![]);
}
