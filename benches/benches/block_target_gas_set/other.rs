use crate::*;

// ECAL
// FLAG
// GM
// GTF

pub fn run_other(group: &mut BenchmarkGroup<WallTime>) {
    // run_group_ref(
    //     &mut c.benchmark_group("flag"),
    //     "flag",
    //     VmBench::new(op::flag(0x10)),
    // );
    run(
        "other/flag",
        group,
        vec![op::flag(0x10), op::jmp(0x10)],
        vec![],
    );
}
