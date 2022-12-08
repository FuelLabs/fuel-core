use std::iter::successors;

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

    let mut linear = vec![1, 10, 100, 1000, 10_000];
    let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
        .take(5)
        .map(|f| f as u64)
        .collect::<Vec<_>>();
    l.sort_unstable();
    linear.extend(l);
    let asset: AssetId = rng.gen();
    let contract: ContractId = rng.gen();

    run_group_ref(
        &mut c.benchmark_group("bal"),
        "bal",
        VmBench::new(Opcode::BAL(0x10, 0x10, 0x11))
            .with_data(asset.iter().chain(contract.iter()).copied().collect())
            .with_prepare_script(vec![
                Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
                Opcode::ADDI(0x11, 0x10, asset.len() as Immediate12),
            ])
            .with_dummy_contract(contract)
            .with_prepare_db(move |mut db| {
                let mut asset_inc = AssetId::zeroed();

                asset_inc.as_mut()[..8].copy_from_slice(&(1 as u64).to_be_bytes());

                db.merkle_contract_asset_id_balance_insert(&contract, &asset_inc, 1)?;

                db.merkle_contract_asset_id_balance_insert(&contract, &asset, 100)?;

                Ok(db)
            }),
    );

    run_group_ref(
        &mut c.benchmark_group("sww"),
        "sww",
        VmBench::contract(rng, Opcode::SWW(REG_ZERO, 0x29, REG_ONE))
            .expect("failed to prepare contract")
            .with_prepare_db(move |mut db| {
                let mut key = Bytes32::zeroed();

                key.as_mut()[..8].copy_from_slice(&(1 as u64).to_be_bytes());

                db.merkle_contract_state_insert(&contract, &key, &key)?;

                Ok(db)
            }),
    );

    let mut scwq = c.benchmark_group("scwq");

    for i in 1..=1 {
        let mut key = Bytes32::zeroed();

        key.as_mut()[..8].copy_from_slice(&(i as u64).to_be_bytes());
        let data = key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
            Opcode::ADDI(0x11, 0x10, ContractId::LEN as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
        ];
        let mut bench = VmBench::contract(rng, Opcode::SCWQ(0x11, 0x29, REG_ONE))
            .expect("failed to prepare contract")
            .with_post_call(post_call)
            .with_prepare_db(move |mut db| {
                db.merkle_contract_state_insert(&contract, &key, &key)?;

                Ok(db)
            });
        bench.data.extend(data);
        run_group_ref(&mut scwq, "scwq", bench);
    }

    scwq.finish();

    let mut call = c.benchmark_group("call");

    for i in linear.clone() {
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

    // FIXME: Currently unable to measure this as it has no inverse and the memory overflows.
    let mut ldc = c.benchmark_group("ldc");

    for i in linear.clone() {
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
            Opcode::MOVI(0x13, i as Immediate18),
        ];

        ldc.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut ldc,
            format!("{}", i),
            VmBench::new(Opcode::LDC(0x10, REG_ZERO, 0x13))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script)
                .with_cleanup(vec![Opcode::RET(REG_ONE)]),
        );
    }

    ldc.finish();

    let mut csiz = c.benchmark_group("csiz");

    for i in linear {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let code = ContractCode::from(code);
        let id = code.id;

        let data = id.iter().copied().collect();

        let prepare_script = vec![Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData)];

        csiz.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut csiz,
            format!("{}", i),
            VmBench::new(Opcode::CSIZ(0x11, 0x10))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    csiz.finish();

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

    run_group_ref(
        &mut c.benchmark_group("mint"),
        "mint",
        VmBench::contract(rng, Opcode::MINT(REG_ZERO))
            .expect("failed to prepare contract"),
    );

    run_group_ref(
        &mut c.benchmark_group("burn"),
        "burn",
        VmBench::contract(rng, Opcode::MINT(REG_ZERO))
            .expect("failed to prepare contract"),
    );

    run_group_ref(
        &mut c.benchmark_group("cb"),
        "cb",
        VmBench::new(Opcode::CB(0x10)).with_prepare_script(vec![
            Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
            Opcode::ALOC(0x10),
            Opcode::ADDI(0x10, REG_HP, 1),
        ]),
    );
}
