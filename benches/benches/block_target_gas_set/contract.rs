use crate::{
    utils::arb_dependent_cost_values,
    *,
};
use fuel_core::service::FuelService;
use fuel_core_storage::{
    tables::ContractsRawCode,
    StorageAsMut,
};
use fuel_core_types::{
    fuel_tx::{
        TxPointer,
        UtxoId,
    },
    fuel_types::Word,
    fuel_vm::consts::WORD_SIZE,
};

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
    let contract_id = ContractId::zeroed();
    let (mut service, rt) = service_with_contract_id(contract_id);
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

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
    let asset_id = AssetId::zeroed();
    let contract_id = ContractId::zeroed();
    let contract_instructions = vec![op::bal(0x13, 0x11, 0x10), op::jmpb(RegId::ZERO, 0)];

    let instructions = vec![
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::movi(0x12, 100_000),
        op::call(0x10, RegId::ZERO, 0x11, 0x12),
    ];

    replace_contract_in_service(&mut service, &contract_id, contract_instructions);

    let script_data: Vec<_> = contract_id
        .iter()
        .copied()
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain(AssetId::default().iter().copied())
        .collect();

    let id = "contract/bal contract";
    run_with_service(
        id,
        group,
        instructions,
        script_data.clone(),
        &service,
        contract_id,
        &rt,
        &mut rng,
    );

    let instructions = vec![
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::movi(0x12, 100_000),
        op::bal(0x13, 0x11, 0x10),
        op::jmpb(RegId::ZERO, 0),
    ];

    let id = "contract/bal script";
    run_with_service(
        id,
        group,
        instructions,
        script_data,
        &service,
        contract_id,
        &rt,
        &mut rng,
    );

    // Call

    for size in arb_dependent_cost_values() {
        let mut contract_instructions = std::iter::repeat(op::noop())
            .take(size as usize)
            .collect::<Vec<_>>();
        contract_instructions.push(op::ret(0x10));

        let instructions = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::movi(0x12, 100_000),
            op::call(0x10, RegId::ZERO, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ];

        replace_contract_in_service(&mut service, &contract_id, contract_instructions);

        let script_data = contract_id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let id = format!("contract/call {:?}", size);
        run_with_service(
            &id,
            group,
            instructions,
            script_data,
            &service,
            contract_id,
            &rt,
            &mut rng,
        );
    }
}

fn replace_contract_in_service(
    service: &mut FuelService,
    contract_id: &ContractId,
    contract_instructions: Vec<Instruction>,
) {
    let contract_bytecode: Vec<_> = contract_instructions
        .iter()
        .map(|x| x.to_bytes())
        .flatten()
        .collect();
    service
        .shared
        .database
        .storage_as_mut::<ContractsRawCode>()
        .insert(contract_id, &contract_bytecode)
        .unwrap();
}
