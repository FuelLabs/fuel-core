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

    // flag
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

    // gtf
    {
        let count = 254;
        let correct_index = count; // Have the last index be the correct one. The builder includes an extra input, so it's the 255th index (254).

        let contract_ids = (0..count)
            .map(|x| ContractId::from([x as u8; 32]))
            .collect::<Vec<_>>();

        let instructions = vec![
            op::movi(0x11, correct_index as u32),
            op::gtf_args(0x10, 0x11, GTFArgs::InputContractOutputIndex),
            op::jmpb(RegId::ZERO, 0),
        ];

        SanityBenchmarkRunnerBuilder::new_with_many_contracts(contract_ids)
            .build()
            .run("other/gtf", group, instructions, vec![]);
    }
}
