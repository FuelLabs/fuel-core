use super::run_group_ref;

use criterion::Criterion;
use fuel_core_benches::*;
use fuel_core_types::fuel_asm::*;

pub fn run(c: &mut Criterion) {
    run_group_ref(
        &mut c.benchmark_group("add"),
        "add",
        VmBench::new(op::add(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("addi"),
        "addi",
        VmBench::new(op::addi(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("aloc"),
        "aloc",
        VmBench::new(op::aloc(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("and"),
        "and",
        VmBench::new(op::and(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("andi"),
        "andi",
        VmBench::new(op::andi(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("div"),
        "div",
        VmBench::new(op::div(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("divi"),
        "divi",
        VmBench::new(op::divi(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("eq"),
        "eq",
        VmBench::new(op::eq(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("exp"),
        "exp",
        VmBench::new(op::exp(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 23), op::movi(0x12, 11)]),
    );

    run_group_ref(
        &mut c.benchmark_group("expi"),
        "expi",
        VmBench::new(op::exp(0x10, 0x11, 11))
            .with_prepare_script(vec![op::movi(0x11, 23)]),
    );

    run_group_ref(
        &mut c.benchmark_group("gt"),
        "gt",
        VmBench::new(op::gt(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    {
        let data = vec![0u8; 32];
        run_group_ref(
            &mut c.benchmark_group("gtf"),
            "gtf",
            VmBench::new(op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData))
                .with_data(data),
        );
    }

    run_group_ref(
        &mut c.benchmark_group("lt"),
        "lt",
        VmBench::new(op::lt(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("mlog"),
        "mlog",
        VmBench::new(op::mlog(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("mod"),
        "mod",
        VmBench::new(op::mod_(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("modi"),
        "modi",
        VmBench::new(op::modi(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("move"),
        "move",
        VmBench::new(op::move_(0x10, 0x11))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("movi"),
        "movi",
        VmBench::new(op::movi(0x10, 27)),
    );

    run_group_ref(
        &mut c.benchmark_group("mroo"),
        "mroo",
        VmBench::new(op::mroo(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("mul"),
        "mul",
        VmBench::new(op::mul(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("muli"),
        "muli",
        VmBench::new(op::muli(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("noop"),
        "noop",
        VmBench::new(op::noop()),
    );

    run_group_ref(
        &mut c.benchmark_group("not"),
        "not",
        VmBench::new(op::not(0x10, 0x11))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("or"),
        "or",
        VmBench::new(op::or(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("ori"),
        "ori",
        VmBench::new(op::ori(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("sll"),
        "sll",
        VmBench::new(op::sll(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("slli"),
        "slli",
        VmBench::new(op::slli(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("srl"),
        "srl",
        VmBench::new(op::srl(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("srli"),
        "srli",
        VmBench::new(op::srli(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("sub"),
        "sub",
        VmBench::new(op::sub(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("subi"),
        "subi",
        VmBench::new(op::subi(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );

    run_group_ref(
        &mut c.benchmark_group("xor"),
        "xor",
        VmBench::new(op::xor(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("xori"),
        "xori",
        VmBench::new(op::xori(0x10, 0x11, 27))
            .with_prepare_script(vec![op::movi(0x11, 100000)]),
    );
}
