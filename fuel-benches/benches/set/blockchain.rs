use super::run_group;

use criterion::Criterion;
use fuel_core_benches::*;
use rand::{
    rngs::StdRng,
    RngCore,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut group = c.benchmark_group("blockchain");

    let cases = vec![1, 10, 100, 1_000, 10_000, 100_000, 1_000_000];
    let asset: AssetId = rng.gen();
    let contract: ContractId = rng.gen();

    for i in cases.clone() {
        run_group(
            &mut group,
            format!("bal ({})", i),
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

    run_group(&mut group, "bhei", VmBench::new(Opcode::BHEI(0x10)));

    run_group(
        &mut group,
        "bhsh",
        VmBench::new(Opcode::BHSH(0x10, REG_ZERO)).with_prepare_script(vec![
            Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
        ]),
    );

    for i in cases.clone() {
        run_group(
            &mut group,
            format!("sww ({})", i),
            VmBench::contract(rng, Opcode::SWW(REG_ZERO, REG_ONE))
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

        run_group(
            &mut group,
            format!("call ({})", i),
            VmBench::new(Opcode::CALL(0x10, REG_ZERO, 0x11, 0x12))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    group.finish();
}
