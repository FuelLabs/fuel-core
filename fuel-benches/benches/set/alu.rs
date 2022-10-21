use super::run_group_ref;

use criterion::Criterion;
use fuel_core_benches::*;

pub fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("alu");

    run_group_ref(
        &mut group,
        "add",
        VmBench::new(Opcode::ADD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "addi",
        VmBench::new(Opcode::ADDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "and",
        VmBench::new(Opcode::AND(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "andi",
        VmBench::new(Opcode::ANDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "div",
        VmBench::new(Opcode::DIV(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "divi",
        VmBench::new(Opcode::DIVI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "eq",
        VmBench::new(Opcode::EQ(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "exp",
        VmBench::new(Opcode::EXP(0x10, 0x11, 0x12))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23), Opcode::MOVI(0x12, 11)]),
    );

    run_group_ref(
        &mut group,
        "expi",
        VmBench::new(Opcode::EXP(0x10, 0x11, 11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23)]),
    );

    run_group_ref(
        &mut group,
        "gt",
        VmBench::new(Opcode::GT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "lt",
        VmBench::new(Opcode::LT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "mlog",
        VmBench::new(Opcode::MLOG(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "mod",
        VmBench::new(Opcode::MOD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "modi",
        VmBench::new(Opcode::MODI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "move",
        VmBench::new(Opcode::MOVE(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(&mut group, "movi", VmBench::new(Opcode::MOVI(0x10, 27)));

    run_group_ref(
        &mut group,
        "mroo",
        VmBench::new(Opcode::MROO(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "mul",
        VmBench::new(Opcode::MUL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "muli",
        VmBench::new(Opcode::MULI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(&mut group, "noop", VmBench::new(Opcode::NOOP));

    run_group_ref(
        &mut group,
        "not",
        VmBench::new(Opcode::NOT(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "or",
        VmBench::new(Opcode::OR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "ori",
        VmBench::new(Opcode::ORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "sll",
        VmBench::new(Opcode::SLL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "slli",
        VmBench::new(Opcode::SLLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "srl",
        VmBench::new(Opcode::SRL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "srli",
        VmBench::new(Opcode::SRLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "sub",
        VmBench::new(Opcode::SUB(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "subi",
        VmBench::new(Opcode::SUBI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut group,
        "xor",
        VmBench::new(Opcode::XOR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut group,
        "xori",
        VmBench::new(Opcode::XORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    group.finish();
}
