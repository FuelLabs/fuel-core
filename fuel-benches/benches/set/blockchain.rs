use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;
use rand::{
    rngs::StdRng,
    RngCore,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let cases = vec![1, 10, 100, 1_000, 10_000, 100_000, 1_000_000];
    // let cases = vec![1_000_000];
    let asset: AssetId = rng.gen();
    let contract: ContractId = rng.gen();

    let mut bal = c.benchmark_group("bal");

    for i in cases.clone() {
        bal.throughput(Throughput::Bytes(i));
        run_group_ref(
            &mut bal,
            format!("{}", i),
            VmBench::new(Opcode::BAL(0x10, 0x10, 0x11))
                .with_data(asset.iter().chain(contract.iter()).copied().collect())
                .with_prepare_script(vec![
                    Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
                    Opcode::ADDI(0x11, 0x10, asset.len() as Immediate12),
                ])
                .with_dummy_contract(contract)
                .with_prepare_db(move |mut db| {
                    let mut asset_inc = AssetId::zeroed();

                    for i in 0..i {
                        asset_inc.as_mut()[..8]
                            .copy_from_slice(&(i as u64).to_be_bytes());

                        db.merkle_contract_asset_id_balance_insert(
                            &contract, &asset_inc, i,
                        )?;
                    }

                    db.merkle_contract_asset_id_balance_insert(&contract, &asset, 100)?;

                    Ok(db)
                }),
        );
    }

    bal.finish();

    let mut sww = c.benchmark_group("sww");
    for i in cases.clone() {
        sww.throughput(Throughput::Bytes(i));
        run_group_ref(
            &mut sww,
            format!("{}", i),
            VmBench::contract(rng, Opcode::SWW(REG_ZERO, 0x29, REG_ONE))
                .expect("failed to prepare contract")
                .with_prepare_db(move |mut db| {
                    let mut key = Bytes32::zeroed();

                    for i in 0..i {
                        key.as_mut()[..8].copy_from_slice(&(i as u64).to_be_bytes());

                        db.merkle_contract_state_insert(&contract, &key, &key)?;
                    }

                    Ok(db)
                }),
        );
    }
    sww.finish();

    let mut call = c.benchmark_group("call");
    let cases = vec![1, 10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000];

    for i in cases {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let code = ContractCode::from(code);
        let id = code.id;

        let data = id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
            Opcode::ADDI(0x11, 0x10, ContractId::LEN as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::MOVI(0x12, 100_000),
        ];

        call.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut call,
            format!("{}", i),
            VmBench::new(Opcode::CALL(0x10, REG_ZERO, 0x11, 0x12))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    call.finish();

    run_group_ref(
        &mut c.benchmark_group("bhei"),
        "bhei",
        VmBench::new(Opcode::BHEI(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("bhsh"),
        "bhsh",
        VmBench::new(Opcode::BHSH(0x10, REG_ZERO)).with_prepare_script(vec![
            Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
        ]),
    );
}
