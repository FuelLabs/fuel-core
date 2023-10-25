use crate::*;
// use crate::utils::arb_dependent_cost_values;

// BAL: Balance of contract ID
// BHEI: Block height
// BHSH: Block hash
// BURN: Burn existing coins
// CALL: Call contract
// CB: Coinbase address
// CCP: Code copy
// CROO: Code Merkle root
// CSIZ: Code size
// LDC: Load code from an external contract
// LOG: Log event
// LOGD: Log data event
// MINT: Mint new coins
// RETD: Return from context with data
// RVRT: Revert
// SMO: Send message to output
// SCWQ: State clear sequential 32 byte slots
// SRW: State read word
// SRWQ: State read sequential 32 byte slots
// SWW: State write word
// SWWQ: State write sequential 32 byte slots
// TIME: Timestamp at height
// TR: Transfer coins to contract
// TRO: Transfer coins to output
pub fn run_contract(_group: &mut BenchmarkGroup<WallTime>) {
    //     run_group_ref(
    //         &mut c.benchmark_group("bal"),
    //         "bal",
    //         VmBench::new(op::bal(0x10, 0x10, 0x11))
    //             .with_data(asset.iter().chain(contract.iter()).copied().collect())
    //             .with_prepare_script(vec![
    //                 op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
    //                 op::addi(0x11, 0x10, asset.len().try_into().unwrap()),
    //             ])
    //             .with_dummy_contract(contract),
    //     );
    // run(
    //     "contract/bal opcode",
    //     group,
    //     [
    //         op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
    //         op::addi(0x11, 0x10, asset.len().try_into().unwrap()),
    //         op::bal(0x10, 0x10, 0x11),
    //         op::jmpb(RegId::ZERO, 0),
    //     ]
    //     .to_vec(),
    //     vec![],
    // );

    // This breaks the benchmarking
    // for i in arb_dependent_cost_values() {
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
