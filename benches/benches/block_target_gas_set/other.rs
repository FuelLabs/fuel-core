use crate::*;

// ECAL
// FLAG
// GM
// GTF

pub fn run_other(group: &mut BenchmarkGroup<WallTime>) {
    let contract_id = ContractId::zeroed();
    let asset_id = AssetId::zeroed();
    let script_data = script_data(&contract_id, &asset_id);
    let mut shared_runner_builder = SanityBenchmarkRunnerBuilder::new_shared(contract_id);
    // run_group_ref(
    //     &mut c.benchmark_group("flag"),
    //     "flag",
    //     VmBench::new(op::flag(0x10)),
    // );
    run(
        "other/flag",
        group,
        vec![op::flag(0x10), op::jmpb(RegId::ZERO, 0)],
        vec![],
    );

    // ecal: Skipped because it would exit the VM

    // gm
    {
        let contract_instructions = vec![op::gm(0x10, 1), op::jmpb(RegId::ZERO, 0)];

        let mut instructions = setup_instructions();
        instructions.extend(vec![op::call(0x10, RegId::ZERO, 0x11, 0x12)]);

        let id = "other/gm";
        shared_runner_builder
            .build_with_new_contract(contract_instructions)
            .run(id, group, instructions, script_data.clone());
    }

    // gtf: TODO: As part of parent issue (https://github.com/FuelLabs/fuel-core/issues/1386)
}
