use std::iter::successors;

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        Input,
        Output,
    },
    fuel_types::*,
    fuel_vm::{
        consts::*,
        InterpreterStorage,
    },
};
use rand::{
    rngs::StdRng,
    RngCore,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let mut linear: Vec<u64> = vec![1, 10, 100, 1000, 10_000];
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

                asset_inc.as_mut()[..8].copy_from_slice(&1_u64.to_be_bytes());

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

                key.as_mut()[..8].copy_from_slice(&1_u64.to_be_bytes());

                db.merkle_contract_state_insert(&contract, &key, &key)?;

                Ok(db)
            }),
    );
    {
        let mut input = VmBench::contract(rng, Opcode::SRW(0x13, 0x14, 0x15))
            .expect("failed to prepare contract")
            .with_prepare_db(move |mut db| {
                let key = Bytes32::zeroed();

                db.merkle_contract_state_insert(&ContractId::zeroed(), &key, &key)?;

                Ok(db)
            });
        input.prepare_script.extend(vec![Opcode::MOVI(0x15, 2000)]);
        run_group_ref(&mut c.benchmark_group("srw"), "srw", input);
    }

    {
        let mut key = Bytes32::zeroed();

        key.as_mut()[..8].copy_from_slice(&1u64.to_be_bytes());
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
        run_group_ref(&mut c.benchmark_group("scwq"), "scwq", bench);
    }

    {
        let mut key = Bytes32::zeroed();

        key.as_mut()[..8].copy_from_slice(&1u64.to_be_bytes());
        let data = key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
            Opcode::ADDI(0x11, 0x10, ContractId::LEN as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
        ];
        let mut bench = VmBench::contract(rng, Opcode::SWWQ(0x10, 0x11, 0x20, REG_ONE))
            .expect("failed to prepare contract")
            .with_post_call(post_call)
            .with_prepare_db(move |mut db| {
                db.merkle_contract_state_insert(&contract, &key, &key)?;

                Ok(db)
            });
        bench.data.extend(data);
        run_group_ref(&mut c.benchmark_group("swwq"), "swwq", bench);
    }

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
            format!("{i}"),
            VmBench::new(Opcode::CALL(0x10, REG_ZERO, 0x11, 0x12))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    call.finish();

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
            format!("{i}"),
            VmBench::new(Opcode::LDC(0x10, REG_ZERO, 0x13))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    ldc.finish();

    let mut ccp = c.benchmark_group("ccp");

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
            Opcode::MOVI(0x14, i as Immediate18),
            Opcode::MOVI(0x15, i as Immediate18),
            Opcode::ADD(0x15, 0x15, 0x15),
            Opcode::ADDI(0x15, 0x15, 32),
            Opcode::ALOC(0x15),
            Opcode::ADDI(0x15, REG_HP, 1),
        ];

        ccp.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut ccp,
            format!("{i}"),
            VmBench::new(Opcode::CCP(0x15, 0x10, REG_ZERO, 0x13))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    ccp.finish();

    let mut csiz = c.benchmark_group("csiz");

    for i in linear.clone() {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let code = ContractCode::from(code);
        let id = code.id;

        let data = id.iter().copied().collect();

        let prepare_script = vec![Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData)];

        csiz.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut csiz,
            format!("{i}"),
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

    {
        let mut input = VmBench::contract(rng, Opcode::TR(0x15, 0x14, 0x15))
            .expect("failed to prepare contract")
            .with_prepare_db(move |mut db| {
                db.merkle_contract_asset_id_balance_insert(
                    &ContractId::zeroed(),
                    &AssetId::zeroed(),
                    200,
                )?;

                Ok(db)
            });
        input
            .prepare_script
            .extend(vec![Opcode::MOVI(0x15, 2000), Opcode::MOVI(0x14, 100)]);
        run_group_ref(&mut c.benchmark_group("tr"), "tr", input);
    }

    {
        let mut input = VmBench::contract(rng, Opcode::TRO(0x15, 0x16, 0x14, 0x15))
            .expect("failed to prepare contract")
            .with_prepare_db(move |mut db| {
                db.merkle_contract_asset_id_balance_insert(
                    &ContractId::zeroed(),
                    &AssetId::zeroed(),
                    200,
                )?;

                Ok(db)
            });
        let coin_output = Output::variable(Address::zeroed(), 100, AssetId::zeroed());
        input.outputs.push(coin_output);
        let predicate = Opcode::RET(REG_ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let coin_input = Input::coin_predicate(
            Default::default(),
            owner,
            1000,
            AssetId::zeroed(),
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        );
        input.inputs.push(coin_input);

        let index = input.outputs.len() - 1;
        input.prepare_script.extend(vec![
            Opcode::MOVI(0x15, 2000),
            Opcode::MOVI(0x14, 100),
            Opcode::MOVI(0x16, index as Immediate18),
        ]);
        run_group_ref(&mut c.benchmark_group("tro"), "tro", input);
    }

    run_group_ref(
        &mut c.benchmark_group("cfsi"),
        "cfsi",
        VmBench::new(Opcode::CFSI(1)),
    );

    {
        let mut input = VmBench::contract(rng, Opcode::CROO(0x14, 0x16))
            .expect("failed to prepare contract");
        input.post_call.extend(vec![
            Opcode::gtf(0x16, 0x00, GTFArgs::ScriptData),
            Opcode::MOVI(0x15, 2000),
            Opcode::ALOC(0x15),
            Opcode::ADDI(0x14, REG_HP, 1),
        ]);
        run_group_ref(&mut c.benchmark_group("croo"), "croo", input);
    }

    run_group_ref(
        &mut c.benchmark_group("flag"),
        "flag",
        VmBench::new(Opcode::FLAG(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("gm"),
        "gm",
        VmBench::contract(rng, Opcode::GM(0x10, 1)).unwrap(),
    );

    let mut smo = c.benchmark_group("smo");

    for i in linear.clone() {
        let mut input = VmBench::contract(rng, Opcode::SMO(0x15, 0x16, 0x17, 0x18))
            .expect("failed to prepare contract");
        input.outputs.push(Output::message(Address::zeroed(), 1));
        let index = input.outputs.len() - 1;
        input.post_call.extend(vec![
            Opcode::gtf(0x15, 0x00, GTFArgs::ScriptData),
            // Offset 32 + 8+ 8 + 32
            Opcode::ADDI(0x15, 0x15, 32 + 8 + 8 + 32),
            Opcode::MOVI(0x16, i as Immediate18),
            Opcode::MOVI(0x17, index as Immediate18),
            Opcode::MOVI(0x18, 10),
        ]);
        input.data.extend(
            Address::new([1u8; 32])
                .iter()
                .copied()
                .chain(vec![2u8; i as usize]),
        );
        let predicate = Opcode::RET(REG_ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let coin_input = Input::coin_predicate(
            Default::default(),
            owner,
            Word::MAX,
            AssetId::zeroed(),
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        );
        input.inputs.push(coin_input);
        smo.throughput(Throughput::Bytes(i));
        run_group_ref(&mut smo, format!("{i}"), input);
    }

    smo.finish();

    let mut srwq = c.benchmark_group("srwq");

    for i in linear.clone() {
        let mut key = Bytes32::zeroed();

        key.as_mut()[..8].copy_from_slice(&i.to_be_bytes());
        let data = key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            Opcode::MOVI(0x16, i as Immediate18),
            Opcode::MOVI(0x17, 2000),
            Opcode::MOVE(0x15, 0x16),
            Opcode::MULI(0x15, 0x15, 32),
            Opcode::ADDI(0x15, 0x15, 1),
            Opcode::ALOC(0x15),
            Opcode::ADDI(0x14, REG_HP, 1),
        ];
        let mut bench = VmBench::contract(rng, Opcode::SRWQ(0x14, 0x11, 0x27, 0x16))
            .expect("failed to prepare contract")
            .with_post_call(post_call)
            .with_prepare_db(move |mut db| {
                let values = vec![key; i as usize];
                db.merkle_contract_state_insert_range(&contract, &key, &values)?;

                Ok(db)
            });
        bench.data.extend(data);
        srwq.throughput(Throughput::Bytes(i));
        run_group_ref(&mut srwq, format!("{i}"), bench);
    }

    srwq.finish();

    run_group_ref(
        &mut c.benchmark_group("time"),
        "time",
        VmBench::new(Opcode::TIME(0x11, 0x10))
            .with_prepare_script(vec![Opcode::MOVI(0x10, 0)]),
    );
}
