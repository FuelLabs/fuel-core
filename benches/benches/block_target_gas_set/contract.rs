use crate::*;
use fuel_core_types::{
    fuel_types::Word,
    fuel_vm::consts::WORD_SIZE,
};
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
pub fn run_contract(group: &mut BenchmarkGroup<WallTime>) {
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

    // let contract_id = ContractId::zeroed();
    // let asset_id = AssetId::zeroed();
    // run(
    //     "contract/bal opcode",
    //     group,
    //     [
    //         op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
    //         op::addi(0x11, 0x10, asset_id.len().try_into().unwrap()),
    //         op::bal(0x10, 0x10, 0x11),
    //         op::jmpb(RegId::ZERO, 0),
    //     ]
    //     .to_vec(),
    //     asset_id
    //         .into_iter()
    //         .chain(contract_id.into_iter())
    //         .collect(),
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

    //     let mut call = c.benchmark_group("call");
    //
    //     for i in linear.clone() {
    //         let mut code = vec![0u8; i as usize];
    //
    //         rng.fill_bytes(&mut code);
    //
    //         let code = ContractCode::from(code);
    //         let id = code.id;
    //
    //         let data = id
    //             .iter()
    //             .copied()
    //             .chain((0 as Word).to_be_bytes().iter().copied())
    //             .chain((0 as Word).to_be_bytes().iter().copied())
    //             .chain(AssetId::default().iter().copied())
    //             .collect();
    //
    //         let prepare_script = vec![
    //             op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
    //             op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
    //             op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
    //             op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
    //             op::movi(0x12, 100_000),
    //         ];
    //
    //         call.throughput(Throughput::Bytes(i));
    //
    //         run_group_ref(
    //             &mut call,
    //             format!("{i}"),
    //             VmBench::new(op::call(0x10, RegId::ZERO, 0x11, 0x12))
    //                 .with_db(db.checkpoint())
    //                 .with_contract_code(code)
    //                 .with_data(data)
    //                 .with_prepare_script(prepare_script),
    //         );
    //     }

    let contract = vec![op::noop(), op::ret(0x10)];

    let instructions = vec![
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::movi(0x12, 100_000),
        op::call(0x10, RegId::ZERO, 0x11, 0x12),
        op::jmpb(RegId::ZERO, 0),
    ];
    let contract_id = ContractId::zeroed();
    let mut service = service_with_contract_id(contract_id);
    let contract_bytecode: Vec<_> =
        contract.iter().map(|x| x.to_bytes()).flatten().collect();
    service
        .shared
        .database
        .insert_contract_bytecode(contract_id, &contract_bytecode)
        .unwrap();

    let script_data = contract_id
        .iter()
        .copied()
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain(AssetId::default().iter().copied())
        .collect();

    run_with_service("contract/call", group, instructions, script_data, service);
}
