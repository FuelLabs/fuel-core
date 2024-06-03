use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use ethnum::U256;
use fuel_core_benches::*;
use fuel_core_types::fuel_asm::*;

use crate::utils::{
    linear_short,
    set_full_word,
};
use fuel_core_types::fuel_asm::wideint::{
    CompareArgs,
    CompareMode,
    DivArgs,
    MathArgs,
    MathOp,
    MulArgs,
};

use super::utils::{
    make_u128,
    make_u256,
};

pub fn run(c: &mut Criterion) {
    let linear_short = linear_short();

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

    // aloc
    {
        let mut aloc = c.benchmark_group("aloc");

        let mut aloc_linear = linear_short.clone();
        aloc_linear.push(1_000_000);
        aloc_linear.push(10_000_000);
        aloc_linear.push(30_000_000);
        for i in aloc_linear {
            let bench =
                VmBench::new(op::aloc(0x10)).with_prepare_script(set_full_word(0x10, i));
            aloc.throughput(Throughput::Bytes(i));
            run_group_ref(&mut aloc, format!("{i}"), bench);
        }

        aloc.finish();
    }

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
        let count = 254;
        let correct_index = count - 1; // Have the last index be the correct one. The builder includes an extra input, so it's the 255th index (254).
        run_group_ref(
            &mut c.benchmark_group("gtf"),
            "gtf",
            VmBench::new(op::gtf_args(0x10, 0x11, GTFArgs::InputContractOutputIndex))
                .with_empty_contracts_count(count)
                .with_prepare_script(vec![op::movi(0x11, correct_index as u32)]),
        );
    }

    run_group_ref(
        &mut c.benchmark_group("lt"),
        "lt",
        VmBench::new(op::lt(0x10, 0x11, 0x12))
            .with_prepare_script(vec![op::movi(0x11, 100000), op::movi(0x12, 27)]),
    );

    run_group_ref(
        &mut c.benchmark_group("mldv"),
        "mldv",
        VmBench::new(op::mldv(0x10, 0x11, 0x12, 0x13)).with_prepare_script(vec![
            op::movi(0x11, 123456),
            op::not(0x12, RegId::ZERO),
            op::movi(0x13, 234567),
        ]),
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

    // Wideint operations: 128 bit
    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u128(0x10, 0));
    wideint_prepare.extend(make_u128(0x11, u128::MAX));
    wideint_prepare.extend(make_u128(0x12, u128::MAX / 2 + 1));
    wideint_prepare.extend(make_u128(0x13, u128::MAX - 158)); // prime
    wideint_prepare.extend(make_u128(0x14, u64::MAX.into()));

    run_group_ref(
        &mut c.benchmark_group("wdcm"),
        "wdcm",
        VmBench::new(op::wdcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wdop"),
        "wdop",
        VmBench::new(op::wdop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wdml"),
        "wdml",
        VmBench::new(op::wdml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wddv"),
        "wddv",
        VmBench::new(op::wddv_args(
            0x10,
            0x12,
            0x13,
            DivArgs { indirect_rhs: true },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wdmd"),
        "wdmd",
        VmBench::new(op::wdmd(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wdam"),
        "wdam",
        VmBench::new(op::wdam(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wdmm"),
        "wdmm",
        VmBench::new(op::wdmm(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );

    // Wideint operations: 256 bit
    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u256(0x10, U256::ZERO));
    wideint_prepare.extend(make_u256(0x11, U256::MAX));
    wideint_prepare.extend(make_u256(0x12, U256::MAX / 2 + 1));
    wideint_prepare.extend(make_u256(0x13, U256::MAX - 188)); // prime
    wideint_prepare.extend(make_u256(0x14, u128::MAX.into()));

    run_group_ref(
        &mut c.benchmark_group("wqcm"),
        "wqcm",
        VmBench::new(op::wqcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqop"),
        "wqop",
        VmBench::new(op::wqop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqml"),
        "wqml",
        VmBench::new(op::wqml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqdv"),
        "wqdv",
        VmBench::new(op::wqdv_args(
            0x10,
            0x12,
            0x13,
            DivArgs { indirect_rhs: true },
        ))
        .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqmd"),
        "wqmd",
        VmBench::new(op::wqmd(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqam"),
        "wqam",
        VmBench::new(op::wqam(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("wqmm"),
        "wqmm",
        VmBench::new(op::wdmm(0x10, 0x12, 0x13, 0x13))
            .with_prepare_script(wideint_prepare.clone()),
    );
}
