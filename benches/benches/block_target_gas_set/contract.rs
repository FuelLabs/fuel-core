use crate::*;
// use crate::utils::generate_linear_costs;

pub fn run_contract(_group: &mut BenchmarkGroup<WallTime>) {
    // This breaks the benchmarking
    // for i in generate_linear_costs() {
    //     let id = format!("flow/retd_contract opcode {:?}", i);
    //     run(
    //         &id,
    //         group,
    //         vec![
    //             op::movi(0x10, i),
    //             op::retd(RegId::ONE, 0x10),
    //             op::jmpb(RegId::ZERO, 0),
    //         ]
    //         .to_vec(),
    //         vec![],
    //     );
    // }
}
