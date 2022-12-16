use super::run_group;

use criterion::Criterion;
use fuel_core_benches::*;
use fuel_core_types::fuel_asm::*;

pub fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("alu");

    run_group(
        &mut group,
        "add",
        VmBench::new(Opcode::ADD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "addi",
        VmBench::new(Opcode::ADDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "and",
        VmBench::new(Opcode::AND(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "andi",
        VmBench::new(Opcode::ANDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "div",
        VmBench::new(Opcode::DIV(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "divi",
        VmBench::new(Opcode::DIVI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "eq",
        VmBench::new(Opcode::EQ(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "exp",
        VmBench::new(Opcode::EXP(0x10, 0x11, 0x12))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23), Opcode::MOVI(0x12, 11)]),
    );

    run_group(
        &mut group,
        "expi",
        VmBench::new(Opcode::EXP(0x10, 0x11, 11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23)]),
    );

    run_group(
        &mut group,
        "gt",
        VmBench::new(Opcode::GT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "lt",
        VmBench::new(Opcode::LT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "mlog",
        VmBench::new(Opcode::MLOG(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "mod",
        VmBench::new(Opcode::MOD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "modi",
        VmBench::new(Opcode::MODI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "move",
        VmBench::new(Opcode::MOVE(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(&mut group, "movi", VmBench::new(Opcode::MOVI(0x10, 27)));

    run_group(
        &mut group,
        "mroo",
        VmBench::new(Opcode::MROO(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "mul",
        VmBench::new(Opcode::MUL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "muli",
        VmBench::new(Opcode::MULI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(&mut group, "noop", VmBench::new(Opcode::NOOP));

    run_group(
        &mut group,
        "not",
        VmBench::new(Opcode::NOT(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "or",
        VmBench::new(Opcode::OR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "ori",
        VmBench::new(Opcode::ORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "sll",
        VmBench::new(Opcode::SLL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "slli",
        VmBench::new(Opcode::SLLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "srl",
        VmBench::new(Opcode::SRL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "srli",
        VmBench::new(Opcode::SRLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "sub",
        VmBench::new(Opcode::SUB(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "subi",
        VmBench::new(Opcode::SUBI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group(
        &mut group,
        "xor",
        VmBench::new(Opcode::XOR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group(
        &mut group,
        "xori",
        VmBench::new(Opcode::XORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    group.finish();
}
