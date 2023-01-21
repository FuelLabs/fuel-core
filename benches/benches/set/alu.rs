use super::run_group_ref;

use criterion::Criterion;
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_vm::consts::REG_ZERO,
};

pub fn run(c: &mut Criterion) {
    run_group_ref(
        &mut c.benchmark_group("add"),
        "add",
        VmBench::new(Opcode::ADD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("addi"),
        "addi",
        VmBench::new(Opcode::ADDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("aloc"),
        "aloc",
        VmBench::new(Opcode::ALOC(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("and"),
        "and",
        VmBench::new(Opcode::AND(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("andi"),
        "andi",
        VmBench::new(Opcode::ANDI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("div"),
        "div",
        VmBench::new(Opcode::DIV(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("divi"),
        "divi",
        VmBench::new(Opcode::DIVI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("eq"),
        "eq",
        VmBench::new(Opcode::EQ(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("exp"),
        "exp",
        VmBench::new(Opcode::EXP(0x10, 0x11, 0x12))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23), Opcode::MOVI(0x12, 11)]),
    );

    run_group_ref(
        &mut c.benchmark_group("expi"),
        "expi",
        VmBench::new(Opcode::EXP(0x10, 0x11, 11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 23)]),
    );

    run_group_ref(
        &mut c.benchmark_group("gt"),
        "gt",
        VmBench::new(Opcode::GT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    {
        let data = vec![0u8; 32];
        run_group_ref(
            &mut c.benchmark_group("gtf"),
            "gtf",
            VmBench::new(Opcode::gtf(0x10, REG_ZERO, GTFArgs::ScriptData))
                .with_data(data),
        );
    }

    run_group_ref(
        &mut c.benchmark_group("lt"),
        "lt",
        VmBench::new(Opcode::LT(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("mlog"),
        "mlog",
        VmBench::new(Opcode::MLOG(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("mod"),
        "mod",
        VmBench::new(Opcode::MOD(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("modi"),
        "modi",
        VmBench::new(Opcode::MODI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("move"),
        "move",
        VmBench::new(Opcode::MOVE(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("movi"),
        "movi",
        VmBench::new(Opcode::MOVI(0x10, 27)),
    );

    run_group_ref(
        &mut c.benchmark_group("mroo"),
        "mroo",
        VmBench::new(Opcode::MROO(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("mul"),
        "mul",
        VmBench::new(Opcode::MUL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("muli"),
        "muli",
        VmBench::new(Opcode::MULI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("noop"),
        "noop",
        VmBench::new(Opcode::NOOP),
    );

    run_group_ref(
        &mut c.benchmark_group("not"),
        "not",
        VmBench::new(Opcode::NOT(0x10, 0x11))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("or"),
        "or",
        VmBench::new(Opcode::OR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("ori"),
        "ori",
        VmBench::new(Opcode::ORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("sll"),
        "sll",
        VmBench::new(Opcode::SLL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("slli"),
        "slli",
        VmBench::new(Opcode::SLLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("srl"),
        "srl",
        VmBench::new(Opcode::SRL(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("srli"),
        "srli",
        VmBench::new(Opcode::SRLI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("sub"),
        "sub",
        VmBench::new(Opcode::SUB(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("subi"),
        "subi",
        VmBench::new(Opcode::SUBI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("xor"),
        "xor",
        VmBench::new(Opcode::XOR(0x10, 0x11, 0x12)).with_prepare_script(vec![
            Opcode::MOVI(0x11, 100000),
            Opcode::MOVI(0x12, 27),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("xori"),
        "xori",
        VmBench::new(Opcode::XORI(0x10, 0x11, 27))
            .with_prepare_script(vec![Opcode::MOVI(0x11, 100000)]),
    );
}
